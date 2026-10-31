//! A reader of one XSD file into the schema model's terms.
//!
//! Every definition is named in the file's target namespace, every
//! `prefix:name` is resolved through the file's own declarations, and every
//! inline type is hoisted under a name derived from its context.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::rc::Rc;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::model::{Declarations, Facets, Method, Name, XSD_NAMESPACE};

#[cfg(test)]
mod tests;

/// The namespace the `xml` prefix is bound to without a declaration.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// An `xsd:attribute`: a declaration, or a reference to a global one.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AttributeDecl {
    pub default: Option<String>,
    pub fixed: Option<String>,
    pub name: Name,
    pub required: bool,
    /// `None` for a reference (`ref=`), whose type is the global attribute's.
    pub type_name: Option<TypeName>,
}

/// A named `xsd:attributeGroup`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AttributeGroupDecl {
    pub attributes: Vec<AttributeItem>,
    pub name: Name,
}

/// One entry of a definition's attributes, in declaration order.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AttributeItem {
    Declaration(AttributeDecl),
    /// An `xsd:attributeGroup ref=`.
    Group(Name),
}

/// An `xsd:complexType`, named or hoisted from an element.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ComplexDecl {
    pub attributes: Vec<AttributeItem>,
    pub content: ContentDecl,
    pub mixed: bool,
    pub name: Name,
}

/// What an `xsd:complexType` declares besides attributes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ContentDecl {
    /// Element content: the particle, if any, after the `xsd:complexContent` base, if any.
    Complex {
        derivation: Option<(Method, Name)>,
        particle: Option<Term>,
    },
    /// `xsd:simpleContent`: text of the base's simple type.
    Simple {
        base: Name,
        method: Method,
        /// The hoisted simple type holding a restriction's facets.
        restricted: Option<Name>,
    },
}

/// A global `xsd:element`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ElementDecl {
    pub name: Name,
    pub type_name: TypeName,
}

/// A named `xsd:group`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct GroupDecl {
    pub name: Name,
    pub particle: Term,
}

/// The definitions of one XSD file, references not yet resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Schema {
    pub attribute_groups: Vec<AttributeGroupDecl>,
    /// Global attributes.
    pub attributes: Vec<AttributeDecl>,
    /// Named complex types and those hoisted from elements.
    pub complex_types: Vec<ComplexDecl>,
    pub declarations: Declarations,
    /// Global elements.
    pub elements: Vec<ElementDecl>,
    pub groups: Vec<GroupDecl>,
    /// The namespace of each `xsd:import`, in order.
    pub imports: Vec<String>,
    /// The target namespace; `""` when the file declares none.
    pub namespace: String,
    pub path: PathBuf,
    /// Prefix to namespace URI, as the root declares them; `""` is the default namespace.
    pub prefixes: BTreeMap<String, String>,
    /// Named simple types and those hoisted from elements, attributes, and other simple types.
    pub simple_types: Vec<SimpleDecl>,
}

impl Schema {
    fn parse(path: &Path, text: &str) -> Result<Self> {
        let root = parse(text)?;
        if root.namespace != XSD_NAMESPACE || root.local != "schema" {
            bail!("the root element is not xsd:schema");
        }
        let mut declarations = Declarations::default();
        count(&root, &mut declarations);
        let mut prefixes = (*root.prefixes).clone();
        prefixes.remove("xml");
        let mut file = File {
            attribute_form: root.form("attributeFormDefault")?,
            element_form: root.form("elementFormDefault")?,
            schema: Self {
                attribute_groups: Vec::new(),
                attributes: Vec::new(),
                complex_types: Vec::new(),
                declarations,
                elements: Vec::new(),
                groups: Vec::new(),
                imports: Vec::new(),
                namespace: root
                    .attribute("targetNamespace")
                    .unwrap_or_default()
                    .to_owned(),
                path: path.to_path_buf(),
                prefixes,
                simple_types: Vec::new(),
            },
            synthetic: BTreeSet::new(),
        };
        for child in root.content() {
            file.definition(child)?;
        }
        Ok(file.schema)
    }

    /// Read the XSD file at `path`.
    ///
    /// # Errors
    ///
    /// The file cannot be read, is not well-formed XML, has no `xsd:schema`
    /// root, or uses a construct the reader does not model (`xsd:include`,
    /// `xsd:redefine`, `xsd:anyAttribute`, `use="prohibited"`, substitution
    /// groups, identity constraints).
    pub fn read<S>(system: &S, path: &Path) -> Result<Self>
    where
        S: System + ?Sized,
    {
        let text = ready_now(system.read_to_string(path))
            .with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(path, &text).with_context(|| format!("cannot read {}", path.display()))
    }
}

/// An `xsd:simpleType`, named or hoisted.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SimpleDecl {
    pub name: Name,
    pub variety: Variety,
}

/// A term of a content model and how often it occurs.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Term {
    pub kind: TermKind,
    /// `None` is `maxOccurs="unbounded"`.
    pub max: Option<u32>,
    pub min: u32,
}

/// The term a [`Term`] repeats.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TermKind {
    All(Vec<Term>),
    Any {
        namespace: String,
        process_contents: String,
    },
    Choice(Vec<Term>),
    /// A local declaration.
    Element {
        name: Name,
        type_name: TypeName,
    },
    /// An `xsd:element ref=`.
    ElementRef(Name),
    /// An `xsd:group ref=`.
    GroupRef(Name),
    Sequence(Vec<Term>),
}

/// The type an element, an attribute, or a simple type names.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct TypeName {
    /// Declared inline and hoisted under a name derived from its context.
    pub anonymous: bool,
    pub name: Name,
}

/// What an `xsd:simpleType` holds.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Variety {
    List(TypeName),
    Restriction {
        base: TypeName,
        enumeration: Vec<String>,
        facets: Facets,
    },
    /// `memberTypes`, then the inline members, in order.
    Union(Vec<TypeName>),
}

/// The file being read and the definitions read so far.
struct File {
    /// `attributeFormDefault="qualified"`.
    attribute_form: bool,
    /// `elementFormDefault="qualified"`.
    element_form: bool,
    schema: Schema,
    /// The names hoisted types have taken.
    synthetic: BTreeSet<String>,
}

impl File {
    fn anonymous_simple(&mut self, tag: &Tag, path: &str) -> Result<TypeName> {
        let name = self.hoisted(path);
        self.simple_type(tag, name.clone())?;
        Ok(TypeName {
            anonymous: true,
            name,
        })
    }

    fn attribute(&mut self, tag: &Tag, owner: &str, global: bool) -> Result<AttributeDecl> {
        if tag.content().any(|child| child.local == "complexType") {
            bail!("xsd:attribute declares a complex type");
        }
        let required = match tag.attribute("use") {
            None | Some("optional") => false,
            Some("required") => true,
            Some(other) => bail!("use=\"{other}\" is not modelled"),
        };
        let default = tag.attribute("default").map(ToOwned::to_owned);
        let fixed = tag.attribute("fixed").map(ToOwned::to_owned);
        if let Some(reference) = tag.attribute("ref") {
            return Ok(AttributeDecl {
                default,
                fixed,
                name: tag.qname(reference)?,
                required,
                type_name: None,
            });
        }
        let local = tag.required("name")?;
        let namespace = if global || tag.qualified(self.attribute_form)? {
            self.schema.namespace.clone()
        } else {
            String::new()
        };
        let type_name = self.type_name(tag, &format!("{owner}/@{local}"), "anySimpleType")?;
        Ok(AttributeDecl {
            default,
            fixed,
            name: Name::new(&namespace, local),
            required,
            type_name: Some(type_name),
        })
    }

    fn attribute_item(&mut self, tag: &Tag, owner: &str) -> Result<AttributeItem> {
        match tag.xsd()? {
            "attribute" => Ok(AttributeItem::Declaration(
                self.attribute(tag, owner, false)?,
            )),
            "attributeGroup" => Ok(AttributeItem::Group(tag.qname(tag.required("ref")?)?)),
            other => bail!("xsd:{other} is not modelled among attributes"),
        }
    }

    fn complex_content(
        &mut self,
        tag: &Tag,
        owner: &str,
        attributes: &mut Vec<AttributeItem>,
    ) -> Result<ContentDecl> {
        let (method, derivation) = tag.derivation()?;
        let base = derivation.qname(derivation.required("base")?)?;
        let mut particle = None;
        for child in derivation.content() {
            match child.xsd()? {
                "all" | "choice" | "group" | "sequence" => {
                    particle = Some(self.term(child, owner)?);
                }
                "attribute" | "attributeGroup" => {
                    attributes.push(self.attribute_item(child, owner)?);
                }
                other => bail!("xsd:{other} is not modelled in complex content"),
            }
        }
        Ok(ContentDecl::Complex {
            derivation: Some((method, base)),
            particle,
        })
    }

    fn complex_type(&mut self, tag: &Tag, name: Name) -> Result<()> {
        let owner = name.local.clone();
        let mut mixed = tag.flag("mixed")?;
        let mut attributes = Vec::new();
        let mut content = ContentDecl::Complex {
            derivation: None,
            particle: None,
        };
        for child in tag.content() {
            match child.xsd()? {
                "all" | "choice" | "group" | "sequence" => {
                    content = ContentDecl::Complex {
                        derivation: None,
                        particle: Some(self.term(child, &owner)?),
                    };
                }
                "attribute" | "attributeGroup" => {
                    attributes.push(self.attribute_item(child, &owner)?);
                }
                "complexContent" => {
                    mixed |= child.flag("mixed")?;
                    content = self.complex_content(child, &owner, &mut attributes)?;
                }
                "simpleContent" => content = self.simple_content(child, &owner, &mut attributes)?,
                other => bail!("xsd:{other} is not modelled in a complex type"),
            }
        }
        self.schema.complex_types.push(ComplexDecl {
            attributes,
            content,
            mixed,
            name,
        });
        Ok(())
    }

    fn definition(&mut self, tag: &Tag) -> Result<()> {
        let namespace = self.schema.namespace.clone();
        match tag.xsd()? {
            "attribute" => {
                let attribute = self.attribute(tag, "", true)?;
                self.schema.attributes.push(attribute);
            }
            "attributeGroup" => {
                let name = Name::new(&namespace, tag.required("name")?);
                let mut attributes = Vec::new();
                for child in tag.content() {
                    attributes.push(self.attribute_item(child, &name.local)?);
                }
                self.schema
                    .attribute_groups
                    .push(AttributeGroupDecl { attributes, name });
            }
            "complexType" => {
                let name = Name::new(&namespace, tag.required("name")?);
                self.complex_type(tag, name.clone())
                    .with_context(|| format!("in {name}"))?;
            }
            "element" => {
                let local = tag.required("name")?;
                tag.refuse_substitution()?;
                let type_name = self.type_name(tag, &format!("/{local}"), "anyType")?;
                self.schema.elements.push(ElementDecl {
                    name: Name::new(&namespace, local),
                    type_name,
                });
            }
            "group" => {
                let name = Name::new(&namespace, tag.required("name")?);
                let mut children = tag.content();
                let (Some(child), None) = (children.next(), children.next()) else {
                    bail!("the group {name} does not hold exactly one particle");
                };
                let particle = self.term(child, &name.local)?;
                self.schema.groups.push(GroupDecl { name, particle });
            }
            "import" => self
                .schema
                .imports
                .push(tag.attribute("namespace").unwrap_or_default().to_owned()),
            "simpleType" => {
                let name = Name::new(&namespace, tag.required("name")?);
                self.simple_type(tag, name.clone())
                    .with_context(|| format!("in {name}"))?;
            }
            other => bail!("xsd:{other} is not modelled"),
        }
        Ok(())
    }

    fn element(&mut self, tag: &Tag, owner: &str) -> Result<TermKind> {
        tag.refuse_substitution()?;
        if let Some(reference) = tag.attribute("ref") {
            return Ok(TermKind::ElementRef(tag.qname(reference)?));
        }
        let local = tag.required("name")?;
        let namespace = if tag.qualified(self.element_form)? {
            self.schema.namespace.clone()
        } else {
            String::new()
        };
        let type_name = self.type_name(tag, &format!("{owner}/{local}"), "anyType")?;
        Ok(TermKind::Element {
            name: Name::new(&namespace, local),
            type_name,
        })
    }

    /// The name of a hoisted type: `path`, or `path#2`, `path#3`, … when
    /// another hoisted type already has it.
    fn hoisted(&mut self, path: &str) -> Name {
        let mut candidate = path.to_owned();
        let mut suffix = 1_u32;
        while !self.synthetic.insert(candidate.clone()) {
            suffix += 1;
            candidate = format!("{path}#{suffix}");
        }
        Name::new(&self.schema.namespace, &candidate)
    }

    fn simple_content(
        &mut self,
        tag: &Tag,
        owner: &str,
        attributes: &mut Vec<AttributeItem>,
    ) -> Result<ContentDecl> {
        let (method, derivation) = tag.derivation()?;
        let base = derivation.qname(derivation.required("base")?)?;
        let mut enumeration = Vec::new();
        let mut facets = Facets::default();
        let mut restricts = false;
        for child in derivation.content() {
            match child.xsd()? {
                "attribute" | "attributeGroup" => {
                    attributes.push(self.attribute_item(child, owner)?);
                }
                facet if method == Method::Restriction => {
                    restricts = true;
                    facet_into(child, facet, &mut enumeration, &mut facets)?;
                }
                other => bail!("xsd:{other} is not modelled in simple content"),
            }
        }
        let restricted = restricts.then(|| {
            let name = self.hoisted(&format!("{owner}/content"));
            self.schema.simple_types.push(SimpleDecl {
                name: name.clone(),
                variety: Variety::Restriction {
                    base: TypeName {
                        anonymous: false,
                        name: base.clone(),
                    },
                    enumeration,
                    facets,
                },
            });
            name
        });
        Ok(ContentDecl::Simple {
            base,
            method,
            restricted,
        })
    }

    fn simple_type(&mut self, tag: &Tag, name: Name) -> Result<()> {
        let owner = name.local.clone();
        let mut children = tag.content();
        let (Some(child), None) = (children.next(), children.next()) else {
            bail!("the simple type {name} does not hold exactly one derivation");
        };
        let variety = match child.xsd()? {
            "list" => Variety::List(if let Some(item) = child.attribute("itemType") {
                TypeName {
                    anonymous: false,
                    name: child.qname(item)?,
                }
            } else {
                let inline = child
                    .content()
                    .next()
                    .context("xsd:list without an item type")?;
                self.anonymous_simple(inline, &format!("{owner}/item"))?
            }),
            "restriction" => {
                let mut base = match child.attribute("base") {
                    Some(base) => Some(TypeName {
                        anonymous: false,
                        name: child.qname(base)?,
                    }),
                    None => None,
                };
                let mut enumeration = Vec::new();
                let mut facets = Facets::default();
                for facet in child.content() {
                    match facet.xsd()? {
                        "simpleType" => {
                            base = Some(self.anonymous_simple(facet, &format!("{owner}/base"))?);
                        }
                        kind => facet_into(facet, kind, &mut enumeration, &mut facets)?,
                    }
                }
                Variety::Restriction {
                    base: base.context("xsd:restriction without a base")?,
                    enumeration,
                    facets,
                }
            }
            "union" => {
                let mut members = Vec::new();
                for member in child
                    .attribute("memberTypes")
                    .unwrap_or_default()
                    .split_whitespace()
                {
                    members.push(TypeName {
                        anonymous: false,
                        name: child.qname(member)?,
                    });
                }
                for (position, inline) in (1_u32..).zip(child.content()) {
                    members.push(self.anonymous_simple(inline, &format!("{owner}/{position}"))?);
                }
                Variety::Union(members)
            }
            other => bail!("xsd:{other} is not modelled in a simple type"),
        };
        self.schema.simple_types.push(SimpleDecl { name, variety });
        Ok(())
    }

    fn term(&mut self, tag: &Tag, owner: &str) -> Result<Term> {
        let (min, max) = tag.occurs()?;
        let kind = match tag.xsd()? {
            "all" => TermKind::All(self.terms(tag, owner)?),
            "any" => TermKind::Any {
                namespace: tag.attribute("namespace").unwrap_or("##any").to_owned(),
                process_contents: tag
                    .attribute("processContents")
                    .unwrap_or("strict")
                    .to_owned(),
            },
            "choice" => TermKind::Choice(self.terms(tag, owner)?),
            "element" => self.element(tag, owner)?,
            "group" => TermKind::GroupRef(tag.qname(tag.required("ref")?)?),
            "sequence" => TermKind::Sequence(self.terms(tag, owner)?),
            other => bail!("xsd:{other} is not modelled in a content model"),
        };
        Ok(Term { kind, max, min })
    }

    fn terms(&mut self, tag: &Tag, owner: &str) -> Result<Vec<Term>> {
        tag.content().map(|child| self.term(child, owner)).collect()
    }

    /// The type `tag` names with `type=`, or the one it declares inline,
    /// hoisted as `path`, or the built-in `default`.
    fn type_name(&mut self, tag: &Tag, path: &str, default: &str) -> Result<TypeName> {
        let mut children = tag.content();
        let (inline, extra) = (children.next(), children.next());
        if let Some(unmodelled) = extra {
            bail!("xsd:{} is not modelled in a declaration", unmodelled.local);
        }
        match (tag.attribute("type"), inline) {
            (Some(named), None) => Ok(TypeName {
                anonymous: false,
                name: tag.qname(named)?,
            }),
            (None, None) => Ok(TypeName {
                anonymous: false,
                name: Name::new(XSD_NAMESPACE, default),
            }),
            (None, Some(declared)) => match declared.xsd()? {
                "complexType" => {
                    let name = self.hoisted(path);
                    self.complex_type(declared, name.clone())?;
                    Ok(TypeName {
                        anonymous: true,
                        name,
                    })
                }
                "simpleType" => self.anonymous_simple(declared, path),
                other => bail!("xsd:{other} is not modelled in a declaration"),
            },
            (Some(_), Some(declared)) => {
                bail!("xsd:{} is not modelled beside type=", declared.local)
            }
        }
    }
}

/// An element of the file, its own prefixes resolved.
struct Tag {
    /// The unprefixed attributes: XML Schema's own.
    attributes: Vec<(String, String)>,
    children: Vec<Self>,
    local: String,
    namespace: String,
    /// The namespace declarations in scope.
    prefixes: Rc<BTreeMap<String, String>>,
}

impl Tag {
    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The children other than `xsd:annotation`.
    fn content(&self) -> impl Iterator<Item = &Self> {
        self.children
            .iter()
            .filter(|child| child.namespace != XSD_NAMESPACE || child.local != "annotation")
    }

    /// The single `xsd:extension` or `xsd:restriction` of a content element.
    fn derivation(&self) -> Result<(Method, &Self)> {
        let mut children = self.content();
        let (Some(child), None) = (children.next(), children.next()) else {
            bail!("xsd:{} does not hold exactly one derivation", self.local);
        };
        match child.xsd()? {
            "extension" => Ok((Method::Extension, child)),
            "restriction" => Ok((Method::Restriction, child)),
            other => bail!("xsd:{other} is not modelled in xsd:{}", self.local),
        }
    }

    fn flag(&self, name: &str) -> Result<bool> {
        match self.attribute(name) {
            None | Some("0" | "false") => Ok(false),
            Some("1" | "true") => Ok(true),
            Some(other) => bail!("{name}=\"{other}\" is not a boolean"),
        }
    }

    /// Whether `attributeFormDefault` or `elementFormDefault` is `qualified`.
    fn form(&self, name: &str) -> Result<bool> {
        match self.attribute(name) {
            None | Some("unqualified") => Ok(false),
            Some("qualified") => Ok(true),
            Some(other) => bail!("{name}=\"{other}\" is not a form"),
        }
    }

    fn new(start: &BytesStart<'_>, scope: &Rc<BTreeMap<String, String>>) -> Result<Self> {
        let mut attributes = Vec::new();
        let mut declared = Vec::new();
        for read in start.attributes() {
            let attribute = read?;
            let key = attribute.key.as_ref();
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)?
                .into_owned();
            if key == "xmlns" {
                declared.push((String::new(), value));
            } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                declared.push((prefix.to_owned(), value));
            } else if !key.contains(':') {
                attributes.push((key.to_owned(), value));
            } else {
                // Attributes in other namespaces annotate the schema; they declare nothing.
            }
        }
        let prefixes = if declared.is_empty() {
            Rc::clone(scope)
        } else {
            let mut in_scope = (**scope).clone();
            in_scope.extend(declared);
            Rc::new(in_scope)
        };
        let qualified = start.name();
        let raw = qualified.as_ref();
        let (prefix, local) = raw.split_once(':').unwrap_or(("", raw));
        let namespace = match prefixes.get(prefix) {
            Some(namespace) => namespace.clone(),
            None if prefix.is_empty() => String::new(),
            None => bail!("the prefix {prefix} is not declared"),
        };
        Ok(Self {
            attributes,
            children: Vec::new(),
            local: local.to_owned(),
            namespace,
            prefixes,
        })
    }

    fn occurs(&self) -> Result<(u32, Option<u32>)> {
        let min = match self.attribute("minOccurs") {
            Some(min) => min
                .parse()
                .with_context(|| format!("minOccurs=\"{min}\""))?,
            None => 1,
        };
        let max = match self.attribute("maxOccurs") {
            Some("unbounded") => None,
            Some(max) => Some(
                max.parse()
                    .with_context(|| format!("maxOccurs=\"{max}\""))?,
            ),
            None => Some(1),
        };
        Ok((min, max))
    }

    /// The name a `prefix:local` value means here; no prefix is the default namespace.
    fn qname(&self, value: &str) -> Result<Name> {
        let (prefix, local) = value.split_once(':').unwrap_or(("", value));
        match self.prefixes.get(prefix) {
            Some(namespace) => Ok(Name::new(namespace, local)),
            None if prefix.is_empty() => Ok(Name::new("", local)),
            None => bail!("the prefix of {value} is not declared"),
        }
    }

    /// Whether a local declaration's name is in the target namespace.
    fn qualified(&self, default: bool) -> Result<bool> {
        match self.attribute("form") {
            None => Ok(default),
            Some("qualified") => Ok(true),
            Some("unqualified") => Ok(false),
            Some(other) => bail!("form=\"{other}\" is not a form"),
        }
    }

    fn refuse_substitution(&self) -> Result<()> {
        if self.attribute("substitutionGroup").is_some() {
            bail!("substitution groups are not modelled");
        }
        Ok(())
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.attribute(name)
            .with_context(|| format!("xsd:{} without {name}", self.local))
    }

    /// The local name of an XML Schema element; anything else is refused.
    fn xsd(&self) -> Result<&str> {
        if self.namespace == XSD_NAMESPACE {
            Ok(&self.local)
        } else {
            bail!(
                "{{{}}}{} is not an XML Schema element",
                self.namespace,
                self.local
            )
        }
    }
}

/// Add to the counts every `xsd:element` and `xsd:attribute` at or under `tag`.
fn count(tag: &Tag, declarations: &mut Declarations) {
    if tag.namespace == XSD_NAMESPACE {
        if tag.local == "attribute" {
            declarations.attributes += 1;
        } else if tag.local == "element" {
            if tag.attribute("ref").is_some() {
                declarations.element_references += 1;
            } else {
                declarations.elements += 1;
            }
        } else {
            // Neither declares an element nor an attribute.
        }
    }
    for child in &tag.children {
        count(child, declarations);
    }
}

/// Record the facet `kind` that `tag` declares.
fn facet_into(
    tag: &Tag,
    kind: &str,
    enumeration: &mut Vec<String>,
    facets: &mut Facets,
) -> Result<()> {
    let value = tag.required("value")?;
    let length = || -> Result<Option<u32>> {
        Ok(Some(value.parse().with_context(|| {
            format!("xsd:{kind} value=\"{value}\"")
        })?))
    };
    match kind {
        "enumeration" => enumeration.push(value.to_owned()),
        "length" => facets.length = length()?,
        "maxExclusive" => facets.max_exclusive = Some(value.to_owned()),
        "maxInclusive" => facets.max_inclusive = Some(value.to_owned()),
        "maxLength" => facets.max_length = length()?,
        "minExclusive" => facets.min_exclusive = Some(value.to_owned()),
        "minInclusive" => facets.min_inclusive = Some(value.to_owned()),
        "minLength" => facets.min_length = length()?,
        "pattern" => facets.patterns.push(value.to_owned()),
        other => bail!("the facet xsd:{other} is not modelled"),
    }
    Ok(())
}

/// The root element of `text`, with every element below it.
fn parse(text: &str) -> Result<Tag> {
    let mut reader = Reader::from_str(text.trim_start_matches('\u{feff}'));
    let predefined = Rc::new(BTreeMap::from([(
        "xml".to_owned(),
        XML_NAMESPACE.to_owned(),
    )]));
    let mut open: Vec<Tag> = Vec::new();
    loop {
        let position = reader.buffer_position();
        let event = reader
            .read_event()
            .with_context(|| format!("malformed XML after byte {position}"))?;
        let closed = match event {
            Event::Start(start) => {
                let tag = Tag::new(
                    &start,
                    open.last().map_or(&predefined, |parent| &parent.prefixes),
                )?;
                open.push(tag);
                None
            }
            Event::Empty(start) => Some(Tag::new(
                &start,
                open.last().map_or(&predefined, |parent| &parent.prefixes),
            )?),
            Event::End(_) => Some(open.pop().context("an end tag closes nothing")?),
            Event::Eof => bail!("the file holds no complete root element"),
            Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::DocType(_)
            | Event::GeneralRef(_)
            | Event::PI(_)
            | Event::Text(_) => None,
        };
        if let Some(tag) = closed {
            match open.last_mut() {
                Some(parent) => parent.children.push(tag),
                None => return Ok(tag),
            }
        }
    }
}
