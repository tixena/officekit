//! The schema model of a set of XSD files: namespaces, global elements and
//! attributes, complex types, simple types, groups, and attribute groups, with
//! every reference resolved across the set's imports.

use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::System;

use super::xsd::{
    AttributeDecl, AttributeGroupDecl, AttributeItem, ComplexDecl, ContentDecl, ElementDecl,
    GroupDecl, Schema, SimpleDecl, Term, TermKind, TypeName, Variety,
};

#[cfg(test)]
mod tests;

/// The XML Schema namespace, whose names are the built-in types.
pub const XSD_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema";

/// The built-in types of XML Schema 1.0.
const BUILT_INS: [&str; 46] = [
    "ENTITIES",
    "ENTITY",
    "ID",
    "IDREF",
    "IDREFS",
    "NCName",
    "NMTOKEN",
    "NMTOKENS",
    "NOTATION",
    "Name",
    "QName",
    "anySimpleType",
    "anyType",
    "anyURI",
    "base64Binary",
    "boolean",
    "byte",
    "date",
    "dateTime",
    "decimal",
    "double",
    "duration",
    "float",
    "gDay",
    "gMonth",
    "gMonthDay",
    "gYear",
    "gYearMonth",
    "hexBinary",
    "int",
    "integer",
    "language",
    "long",
    "negativeInteger",
    "nonNegativeInteger",
    "nonPositiveInteger",
    "normalizedString",
    "positiveInteger",
    "short",
    "string",
    "time",
    "token",
    "unsignedByte",
    "unsignedInt",
    "unsignedLong",
    "unsignedShort",
];

/// A named set of attributes, its `xsd:attributeGroup` references expanded.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AttributeGroup {
    /// Its own attributes and those of the groups it references, in declaration order.
    pub attributes: Vec<AttributeUse>,
    pub name: Name,
}

/// An attribute a definition allows.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AttributeUse {
    pub default: Option<String>,
    pub fixed: Option<String>,
    /// The `xsd:attributeGroup` reference that brought it into the definition
    /// declaring it; `None` for the definition's own attributes.
    pub group: Option<Name>,
    pub name: Name,
    pub required: bool,
    pub type_ref: TypeRef,
}

/// A complex type as its definition declares it; [`SchemaSet::effective`]
/// folds in its base.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ComplexType {
    /// Own and from `xsd:attributeGroup` references, in declaration order.
    pub attributes: Vec<AttributeUse>,
    /// The `xsd:complexContent` or `xsd:simpleContent` extension or restriction.
    pub base: Option<Derivation>,
    pub content: Content,
    /// `mixed="true"`: text may sit between the child elements.
    pub mixed: bool,
    pub name: Name,
}

/// What a complex type holds besides attributes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Content {
    Empty,
    Particle(Particle),
    /// Text of this simple type.
    Simple(TypeRef),
}

/// How many `xsd:element` and `xsd:attribute` tags a file holds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Declarations {
    /// `xsd:attribute` tags, declarations and references alike.
    pub attributes: usize,
    /// `xsd:element` tags with `ref=`.
    pub element_references: usize,
    /// `xsd:element` tags with `name=`.
    pub elements: usize,
}

/// The base a complex type derives from, and how.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Derivation {
    pub base: TypeRef,
    pub method: Method,
}

/// A global element.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Element {
    pub name: Name,
    pub type_ref: TypeRef,
}

/// The facets of an `xsd:restriction` other than `xsd:enumeration`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Facets {
    pub length: Option<u32>,
    pub max_exclusive: Option<String>,
    pub max_inclusive: Option<String>,
    pub max_length: Option<u32>,
    pub min_exclusive: Option<String>,
    pub min_inclusive: Option<String>,
    pub min_length: Option<u32>,
    /// Each `xsd:pattern`, in order; a value matches when it matches one.
    pub patterns: Vec<String>,
}

/// A named model group.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Group {
    pub name: Name,
    /// Its `xsd:sequence`, `xsd:choice`, or `xsd:all`.
    pub particle: Particle,
}

/// `xsd:extension` or `xsd:restriction`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Method {
    Extension,
    Restriction,
}

/// A namespace URI and the local name of a definition in it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct Name {
    pub local: String,
    pub namespace: String,
}

impl Name {
    #[must_use]
    pub fn new(namespace: &str, local: &str) -> Self {
        Self {
            local: local.to_owned(),
            namespace: namespace.to_owned(),
        }
    }
}

impl fmt::Display for Name {
    /// Clark notation: `{namespace}local`, or `local` in no namespace.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace.is_empty() {
            f.write_str(&self.local)
        } else {
            write!(f, "{{{}}}{}", self.namespace, self.local)
        }
    }
}

/// A term of a content model and how often it occurs.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Particle {
    pub kind: ParticleKind,
    /// `None` is `maxOccurs="unbounded"`.
    pub max: Option<u32>,
    pub min: u32,
}

impl fmt::Display for Particle {
    /// A compact content model: `(a, b)` a sequence, `(a | b)` a choice,
    /// `all(a, b)`, `name: type` an element, then `?`, `*`, `+`, or `{min,max}`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParticleKind::All(children) => list(f, "all(", ", ", children)?,
            ParticleKind::Any {
                namespace,
                process_contents,
            } => write!(f, "any({namespace}, {process_contents})")?,
            ParticleKind::Choice(children) => list(f, "(", " | ", children)?,
            ParticleKind::Element { name, type_ref } => write!(f, "{name}: {type_ref}")?,
            ParticleKind::GroupRef(name) => write!(f, "group {name}")?,
            ParticleKind::Sequence(children) => list(f, "(", ", ", children)?,
        }
        match (self.min, self.max) {
            (1, Some(1)) => Ok(()),
            (0, Some(1)) => f.write_str("?"),
            (0, None) => f.write_str("*"),
            (1, None) => f.write_str("+"),
            (min, None) => write!(f, "{{{min},}}"),
            (min, Some(max)) => write!(f, "{{{min},{max}}}"),
        }
    }
}

/// The term a particle repeats.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParticleKind {
    All(Vec<Particle>),
    Any {
        namespace: String,
        process_contents: String,
    },
    Choice(Vec<Particle>),
    /// A local declaration or a reference to a global element.
    Element {
        name: Name,
        type_ref: TypeRef,
    },
    GroupRef(Name),
    Sequence(Vec<Particle>),
}

/// One XSD file of a set.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SchemaFile {
    pub declarations: Declarations,
    /// The namespace of each `xsd:import`, in order.
    pub imports: Vec<String>,
    pub path: PathBuf,
    /// Prefix to namespace URI, as the root declares them; `""` is the default namespace.
    pub prefixes: BTreeMap<String, String>,
}

/// The schema model of a set of XSD files.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct SchemaSet {
    pub attribute_groups: BTreeMap<Name, AttributeGroup>,
    /// Global attributes.
    pub attributes: BTreeMap<Name, AttributeUse>,
    pub complex_types: BTreeMap<Name, ComplexType>,
    /// Global elements.
    pub elements: BTreeMap<Name, Element>,
    pub groups: BTreeMap<Name, Group>,
    /// Target namespace to the file that declares it.
    pub namespaces: BTreeMap<String, SchemaFile>,
    pub simple_types: BTreeMap<Name, SimpleType>,
}

impl SchemaSet {
    fn check_bases(&self) -> Result<()> {
        for complex_type in self.complex_types.values() {
            let mut seen = BTreeSet::from([&complex_type.name]);
            let mut current = complex_type;
            while let Some(Derivation {
                base:
                    TypeRef::Complex {
                        anonymous: _anonymous,
                        name,
                    },
                method: _method,
            }) = &current.base
            {
                if !seen.insert(name) {
                    bail!("the bases of {} form a cycle", complex_type.name);
                }
                let Some(base) = self.complex_types.get(name) else {
                    break;
                };
                current = base;
            }
        }
        Ok(())
    }

    /// The `xsd:element` and `xsd:attribute` tags of every file, summed.
    #[must_use]
    pub fn declarations(&self) -> Declarations {
        self.namespaces
            .values()
            .fold(Declarations::default(), |sum, file| Declarations {
                attributes: sum.attributes + file.declarations.attributes,
                element_references: sum.element_references + file.declarations.element_references,
                elements: sum.elements + file.declarations.elements,
            })
    }

    /// `complex_type` with its base folded in: for an extension, the base's
    /// attributes then its own, and the base's particle then its own in a
    /// sequence; for a restriction, the base's attributes as it redeclares
    /// them, and its own content.
    #[must_use]
    pub fn effective(&self, complex_type: &ComplexType) -> ComplexType {
        let mut flat = complex_type.clone();
        let Some(derivation) = &complex_type.base else {
            return flat;
        };
        let TypeRef::Complex {
            anonymous: _anonymous,
            name,
        } = &derivation.base
        else {
            return flat;
        };
        let Some(base) = self.complex_types.get(name) else {
            return flat;
        };
        let parent = self.effective(base);
        match derivation.method {
            Method::Extension => {
                flat.attributes = parent.attributes;
                flat.attributes
                    .extend(complex_type.attributes.iter().cloned());
                flat.content = extend(parent.content, &complex_type.content);
            }
            Method::Restriction => {
                let mut attributes: Vec<AttributeUse> = parent
                    .attributes
                    .into_iter()
                    .map(|inherited| {
                        complex_type
                            .attributes
                            .iter()
                            .find(|own| own.name == inherited.name)
                            .cloned()
                            .unwrap_or(inherited)
                    })
                    .collect();
                for own in &complex_type.attributes {
                    if !attributes.iter().any(|kept| kept.name == own.name) {
                        attributes.push(own.clone());
                    }
                }
                flat.attributes = attributes;
            }
        }
        flat
    }

    /// Every distinct pair of element tag and type the set declares: its
    /// global elements and the elements of its complex types and groups.
    #[must_use]
    pub fn element_tags(&self) -> BTreeSet<(&Name, &TypeRef)> {
        let mut tags: BTreeSet<(&Name, &TypeRef)> = self
            .elements
            .values()
            .map(|element| (&element.name, &element.type_ref))
            .collect();
        for particle in self.particles() {
            if let ParticleKind::Element { name, type_ref } = &particle.1.kind {
                tags.insert((name, type_ref));
            }
        }
        tags
    }

    /// Every particle of every complex type and group, nested ones included,
    /// with the definition it sits in; group references are not followed.
    fn particles(&self) -> Vec<(&Name, &Particle)> {
        let mut found = Vec::new();
        for complex_type in self.complex_types.values() {
            if let Content::Particle(particle) = &complex_type.content {
                collect(&complex_type.name, particle, &mut found);
            }
        }
        for group in self.groups.values() {
            collect(&group.name, &group.particle, &mut found);
        }
        found
    }

    /// Read every file of `paths` and resolve the references between them.
    ///
    /// A reference into a namespace one of the files declares must name a
    /// definition there; one into any other namespace is kept as
    /// [`TypeRef::Outside`] or [`TypeRef::Unknown`].
    ///
    /// # Errors
    ///
    /// A file that cannot be read or is not an XSD the reader knows, two files
    /// with one target namespace, a definition declared twice, a reference
    /// that resolves to nothing, or a cycle of attribute groups or bases.
    pub fn read<S, P>(system: &S, paths: &[P]) -> Result<Self>
    where
        S: System + ?Sized,
        P: AsRef<Path>,
    {
        let mut schemas = Vec::with_capacity(paths.len());
        for path in paths {
            schemas.push(Schema::read(system, path.as_ref())?);
        }
        let index = Index::new(&schemas)?;
        let mut set = Self::default();
        for schema in &schemas {
            index
                .resolve(schema, &mut set)
                .with_context(|| format!("cannot resolve {}", schema.path.display()))?;
        }
        set.check_bases()?;
        Ok(set)
    }

    /// Every wildcard (`xsd:any`) particle, with the complex type or group it sits in.
    #[must_use]
    pub fn wildcards(&self) -> Vec<(&Name, &Particle)> {
        self.particles()
            .into_iter()
            .filter(|(_, particle)| {
                matches!(
                    &particle.kind,
                    ParticleKind::Any {
                        namespace: _namespace,
                        process_contents: _process_contents
                    }
                )
            })
            .collect()
    }
}

/// A simple type.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SimpleType {
    List {
        item: TypeRef,
        name: Name,
    },
    Restriction {
        base: TypeRef,
        enumeration: Vec<String>,
        facets: Facets,
        name: Name,
    },
    Union {
        members: Vec<TypeRef>,
        name: Name,
    },
}

impl SimpleType {
    #[must_use]
    pub const fn name(&self) -> &Name {
        match self {
            Self::List { item: _item, name } => name,
            Self::Restriction {
                base: _base,
                enumeration: _enumeration,
                facets: _facets,
                name,
            } => name,
            Self::Union {
                members: _members,
                name,
            } => name,
        }
    }
}

/// The type of an element, an attribute, or a simple type's base, item, or member.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum TypeRef {
    /// An XML Schema built-in, by local name: `string` for `xsd:string`.
    BuiltIn(String),
    /// A complex type of the set; `anonymous` when it was declared inline and
    /// named after its context.
    Complex { anonymous: bool, name: Name },
    /// A named type in a namespace no file of the set declares.
    Outside(Name),
    /// A simple type of the set; `anonymous` as for `Complex`.
    Simple { anonymous: bool, name: Name },
    /// The type of a referenced element or attribute that lies in a namespace
    /// no file of the set declares.
    Unknown,
}

impl TypeRef {
    /// The name of the type, `xsd:` built-ins in [`XSD_NAMESPACE`]; `None` when unknown.
    #[must_use]
    pub fn name(&self) -> Option<Name> {
        match self {
            Self::BuiltIn(local) => Some(Name::new(XSD_NAMESPACE, local)),
            Self::Complex {
                anonymous: _anonymous,
                name,
            }
            | Self::Simple {
                anonymous: _anonymous,
                name,
            } => Some(name.clone()),
            Self::Outside(name) => Some(name.clone()),
            Self::Unknown => None,
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(local) => write!(f, "xsd:{local}"),
            Self::Complex {
                anonymous: _anonymous,
                name,
            }
            | Self::Simple {
                anonymous: _anonymous,
                name,
            } => write!(f, "{name}"),
            Self::Outside(name) => write!(f, "{name}"),
            Self::Unknown => f.write_str("(unknown)"),
        }
    }
}

/// The raw definitions of every file of a set, by name, for resolving references.
struct Index<'set> {
    attribute_groups: BTreeMap<Name, &'set AttributeGroupDecl>,
    attributes: BTreeMap<Name, &'set AttributeDecl>,
    complex_types: BTreeMap<Name, &'set ComplexDecl>,
    elements: BTreeMap<Name, &'set ElementDecl>,
    groups: BTreeMap<Name, &'set GroupDecl>,
    namespaces: BTreeSet<&'set str>,
    simple_types: BTreeMap<Name, &'set SimpleDecl>,
}

impl<'set> Index<'set> {
    fn attribute(&self, decl: &AttributeDecl) -> Result<AttributeUse> {
        let Some(type_name) = &decl.type_name else {
            if !self.holds(&decl.name) {
                return Ok(AttributeUse {
                    default: decl.default.clone(),
                    fixed: decl.fixed.clone(),
                    group: None,
                    name: decl.name.clone(),
                    required: decl.required,
                    type_ref: TypeRef::Unknown,
                });
            }
            let global = self
                .attributes
                .get(&decl.name)
                .with_context(|| format!("no global attribute {}", decl.name))?;
            let global_type = global
                .type_name
                .as_ref()
                .with_context(|| format!("the global attribute {} has no type", decl.name))?;
            return Ok(AttributeUse {
                default: decl.default.clone().or_else(|| global.default.clone()),
                fixed: decl.fixed.clone().or_else(|| global.fixed.clone()),
                group: None,
                name: decl.name.clone(),
                required: decl.required,
                type_ref: self.type_ref(global_type)?,
            });
        };
        Ok(AttributeUse {
            default: decl.default.clone(),
            fixed: decl.fixed.clone(),
            group: None,
            name: decl.name.clone(),
            required: decl.required,
            type_ref: self.type_ref(type_name)?,
        })
    }

    fn attribute_group(&self, name: &Name, stack: &mut Vec<Name>) -> Result<Vec<AttributeUse>> {
        if stack.contains(name) {
            bail!("the attribute group {name} refers to itself");
        }
        let decl = self
            .attribute_groups
            .get(name)
            .with_context(|| format!("no attribute group {name}"))?;
        stack.push(name.clone());
        let uses = self.attributes(&decl.attributes, stack);
        stack.pop();
        uses
    }

    /// The attribute uses of `items`, attribute groups expanded; `stack`
    /// holds the attribute groups being expanded, to refuse a cycle.
    fn attributes(
        &self,
        items: &[AttributeItem],
        stack: &mut Vec<Name>,
    ) -> Result<Vec<AttributeUse>> {
        let mut uses = Vec::new();
        for item in items {
            match item {
                AttributeItem::Declaration(decl) => uses.push(self.attribute(decl)?),
                AttributeItem::Group(name) => {
                    for mut inherited in self.attribute_group(name, stack)? {
                        inherited.group = Some(name.clone());
                        uses.push(inherited);
                    }
                }
            }
        }
        Ok(uses)
    }

    fn complex_type(&self, decl: &ComplexDecl) -> Result<ComplexType> {
        let attributes = self.attributes(&decl.attributes, &mut Vec::new())?;
        let (base, content) = match &decl.content {
            ContentDecl::Complex {
                derivation,
                particle,
            } => {
                let base = match derivation {
                    Some((method, base)) => Some(Derivation {
                        base: self.named_type(base, false)?,
                        method: *method,
                    }),
                    None => None,
                };
                let content = match particle {
                    Some(term) => Content::Particle(self.particle(term)?),
                    None => Content::Empty,
                };
                (base, content)
            }
            ContentDecl::Simple {
                base,
                method,
                restricted,
            } => {
                let content_type = match restricted {
                    Some(restriction) => TypeRef::Simple {
                        anonymous: true,
                        name: restriction.clone(),
                    },
                    None => self.simple_content(base, &mut Vec::new())?,
                };
                let derivation = Derivation {
                    base: self.named_type(base, false)?,
                    method: *method,
                };
                (Some(derivation), Content::Simple(content_type))
            }
        };
        Ok(ComplexType {
            attributes,
            base,
            content,
            mixed: decl.mixed,
            name: decl.name.clone(),
        })
    }

    fn holds(&self, name: &Name) -> bool {
        self.namespaces.contains(name.namespace.as_str())
    }

    fn named_type(&self, name: &Name, anonymous: bool) -> Result<TypeRef> {
        if name.namespace == XSD_NAMESPACE {
            if BUILT_INS.contains(&name.local.as_str()) {
                return Ok(TypeRef::BuiltIn(name.local.clone()));
            }
            bail!("{name} is not an XML Schema built-in type");
        }
        if !self.holds(name) {
            return Ok(TypeRef::Outside(name.clone()));
        }
        if self.complex_types.contains_key(name) {
            return Ok(TypeRef::Complex {
                anonymous,
                name: name.clone(),
            });
        }
        if self.simple_types.contains_key(name) {
            return Ok(TypeRef::Simple {
                anonymous,
                name: name.clone(),
            });
        }
        bail!("no type {name}")
    }

    fn new(schemas: &'set [Schema]) -> Result<Self> {
        let mut index = Self {
            attribute_groups: BTreeMap::new(),
            attributes: BTreeMap::new(),
            complex_types: BTreeMap::new(),
            elements: BTreeMap::new(),
            groups: BTreeMap::new(),
            namespaces: BTreeSet::new(),
            simple_types: BTreeMap::new(),
        };
        for schema in schemas {
            if !index.namespaces.insert(&schema.namespace) {
                bail!(
                    "{} declares the target namespace {:?}, which another file of the set declares",
                    schema.path.display(),
                    schema.namespace
                );
            }
            insert(
                &mut index.attribute_groups,
                &schema.attribute_groups,
                |decl| &decl.name,
            )?;
            insert(&mut index.attributes, &schema.attributes, |decl| &decl.name)?;
            insert(&mut index.complex_types, &schema.complex_types, |decl| {
                &decl.name
            })?;
            insert(&mut index.elements, &schema.elements, |decl| &decl.name)?;
            insert(&mut index.groups, &schema.groups, |decl| &decl.name)?;
            insert(&mut index.simple_types, &schema.simple_types, |decl| {
                &decl.name
            })?;
        }
        for name in index.complex_types.keys() {
            if index.simple_types.contains_key(name) {
                bail!("{name} is both a complex type and a simple type");
            }
        }
        Ok(index)
    }

    fn particle(&self, term: &Term) -> Result<Particle> {
        let kind = match &term.kind {
            TermKind::All(terms) => ParticleKind::All(self.particles(terms)?),
            TermKind::Any {
                namespace,
                process_contents,
            } => ParticleKind::Any {
                namespace: namespace.clone(),
                process_contents: process_contents.clone(),
            },
            TermKind::Choice(terms) => ParticleKind::Choice(self.particles(terms)?),
            TermKind::Element { name, type_name } => ParticleKind::Element {
                name: name.clone(),
                type_ref: self.type_ref(type_name)?,
            },
            TermKind::ElementRef(name) => {
                let type_ref = if self.holds(name) {
                    let global = self
                        .elements
                        .get(name)
                        .with_context(|| format!("no global element {name}"))?;
                    self.type_ref(&global.type_name)?
                } else {
                    TypeRef::Unknown
                };
                ParticleKind::Element {
                    name: name.clone(),
                    type_ref,
                }
            }
            TermKind::GroupRef(name) => {
                if self.holds(name) && !self.groups.contains_key(name) {
                    bail!("no group {name}");
                }
                ParticleKind::GroupRef(name.clone())
            }
            TermKind::Sequence(terms) => ParticleKind::Sequence(self.particles(terms)?),
        };
        Ok(Particle {
            kind,
            max: term.max,
            min: term.min,
        })
    }

    fn particles(&self, terms: &[Term]) -> Result<Vec<Particle>> {
        terms.iter().map(|term| self.particle(term)).collect()
    }

    fn resolve(&self, schema: &Schema, set: &mut SchemaSet) -> Result<()> {
        for decl in &schema.attribute_groups {
            let attributes = self.attribute_group(&decl.name, &mut Vec::new())?;
            set.attribute_groups.insert(
                decl.name.clone(),
                AttributeGroup {
                    attributes,
                    name: decl.name.clone(),
                },
            );
        }
        for decl in &schema.attributes {
            set.attributes
                .insert(decl.name.clone(), self.attribute(decl)?);
        }
        for decl in &schema.complex_types {
            let complex_type = self.complex_type(decl)?;
            set.complex_types.insert(decl.name.clone(), complex_type);
        }
        for decl in &schema.elements {
            let element = Element {
                name: decl.name.clone(),
                type_ref: self.type_ref(&decl.type_name)?,
            };
            set.elements.insert(decl.name.clone(), element);
        }
        for decl in &schema.groups {
            let group = Group {
                name: decl.name.clone(),
                particle: self.particle(&decl.particle)?,
            };
            set.groups.insert(decl.name.clone(), group);
        }
        for decl in &schema.simple_types {
            set.simple_types
                .insert(decl.name.clone(), self.simple_type(decl)?);
        }
        set.namespaces.insert(
            schema.namespace.clone(),
            SchemaFile {
                declarations: schema.declarations,
                imports: schema.imports.clone(),
                path: schema.path.clone(),
                prefixes: schema.prefixes.clone(),
            },
        );
        Ok(())
    }

    /// The simple type of the text a complex type of simple content holds,
    /// following `name` through complex bases; `stack` refuses a cycle.
    fn simple_content(&self, name: &Name, stack: &mut Vec<Name>) -> Result<TypeRef> {
        let Some(decl) = self.complex_types.get(name) else {
            return self.named_type(name, false);
        };
        if stack.contains(name) {
            bail!("the simple content of {name} refers to itself");
        }
        let ContentDecl::Simple {
            base,
            method: _method,
            restricted,
        } = &decl.content
        else {
            bail!("{name} is the base of simple content but has none");
        };
        if let Some(restriction) = restricted {
            return Ok(TypeRef::Simple {
                anonymous: true,
                name: restriction.clone(),
            });
        }
        stack.push(name.clone());
        let content = self.simple_content(base, stack);
        stack.pop();
        content
    }

    fn simple_type(&self, decl: &SimpleDecl) -> Result<SimpleType> {
        let name = decl.name.clone();
        Ok(match &decl.variety {
            Variety::List(item) => SimpleType::List {
                item: self.type_ref(item)?,
                name,
            },
            Variety::Restriction {
                base,
                enumeration,
                facets,
            } => SimpleType::Restriction {
                base: if self.complex_types.contains_key(&base.name) {
                    self.simple_content(&base.name, &mut Vec::new())?
                } else {
                    self.type_ref(base)?
                },
                enumeration: enumeration.clone(),
                facets: facets.clone(),
                name,
            },
            Variety::Union(members) => SimpleType::Union {
                members: members
                    .iter()
                    .map(|member| self.type_ref(member))
                    .collect::<Result<_>>()?,
                name,
            },
        })
    }

    fn type_ref(&self, type_name: &TypeName) -> Result<TypeRef> {
        self.named_type(&type_name.name, type_name.anonymous)
    }
}

/// Every particle under `particle`, itself included, paired with `owner`.
fn collect<'set>(
    owner: &'set Name,
    particle: &'set Particle,
    found: &mut Vec<(&'set Name, &'set Particle)>,
) {
    found.push((owner, particle));
    match &particle.kind {
        ParticleKind::All(children)
        | ParticleKind::Choice(children)
        | ParticleKind::Sequence(children) => {
            for child in children {
                collect(owner, child, found);
            }
        }
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => {}
        ParticleKind::Element {
            name: _name,
            type_ref: _type_ref,
        } => {}
        ParticleKind::GroupRef(_) => {}
    }
}

/// The content of an extension: the base's particle, then the extension's own.
fn extend(base: Content, own: &Content) -> Content {
    match (base, own) {
        (Content::Particle(inherited), Content::Particle(declared)) => {
            Content::Particle(Particle {
                kind: ParticleKind::Sequence(vec![inherited, declared.clone()]),
                max: Some(1),
                min: 1,
            })
        }
        (inherited, Content::Empty) => inherited,
        (Content::Empty | Content::Simple(_), Content::Particle(_))
        | (Content::Empty | Content::Particle(_) | Content::Simple(_), Content::Simple(_)) => {
            own.clone()
        }
    }
}

fn insert<'set, T, F>(index: &mut BTreeMap<Name, &'set T>, decls: &'set [T], name: F) -> Result<()>
where
    F: Fn(&T) -> &Name,
{
    for decl in decls {
        let key = name(decl);
        if index.insert(key.clone(), decl).is_some() {
            bail!("{key} is declared twice");
        }
    }
    Ok(())
}

/// `children` between `open` and `)`, apart by `separator`.
fn list(
    f: &mut fmt::Formatter<'_>,
    open: &str,
    separator: &str,
    children: &[Particle],
) -> fmt::Result {
    f.write_str(open)?;
    for (position, child) in children.iter().enumerate() {
        if position > 0 {
            f.write_str(separator)?;
        }
        write!(f, "{child}")?;
    }
    f.write_str(")")
}
