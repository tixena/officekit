//! Qualified names: a namespace id and a local name.

#[cfg(feature = "binary-format")]
mod archive;
pub mod table;

use alloc::sync::Arc;

use table::NAMESPACES;

use super::conformance::ConformanceClass;

#[cfg(test)]
mod tests;

/// Index into the generated namespace table.
pub type NamespaceId = u8;

/// An Office version officekit validates against, Office 2010 to Microsoft 365.
/// With `binary-format`, rkyv archives it through its mirror in `archive`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum OfficeVersion {
    /// The current Microsoft 365 release.
    Microsoft365,
    /// Office 2010.
    Office2010,
    /// Office 2013.
    Office2013,
    /// Office 2016.
    Office2016,
    /// Office 2019.
    Office2019,
    /// Office 2021.
    Office2021,
}

/// A name's namespace: none, one the table knows by either class's URI, or any
/// other URI kept as written. With `binary-format`, rkyv archives it through
/// its mirror in `archive`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Namespace {
    /// A namespace in the table, by its id. Its Transitional and Strict URIs are the same value.
    Known(NamespaceId),
    /// No namespace: an unprefixed attribute, or an element with no default namespace.
    None,
    /// A URI the table does not know, kept as written.
    Other(Arc<str>),
}

/// A qualified name: the prefix as the part wrote it, and the namespace it
/// stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
#[non_exhaustive]
pub struct QName {
    /// The name after the colon.
    pub local: String,
    /// The namespace the prefix stands for.
    pub namespace: Namespace,
    /// The prefix as written; empty when there is none.
    pub prefix: String,
}

impl Namespace {
    /// The table's id for a Transitional or a Strict URI, else the URI itself;
    /// the empty URI is `None`.
    #[must_use]
    pub fn from_uri(uri: &str) -> Self {
        if uri.is_empty() {
            return Self::None;
        }
        NAMESPACES
            .iter()
            .position(|row| row.uri == uri || row.strict_uri == Some(uri))
            .and_then(|index| NamespaceId::try_from(index).ok())
            .map_or_else(|| Self::Other(Arc::from(uri)), Self::Known)
    }

    /// The URI to write in `class`: the Strict URI of a known namespace that
    /// has one, else its Transitional URI.
    #[must_use]
    pub fn uri(&self, class: ConformanceClass) -> &str {
        match self {
            Self::Known(id) => NAMESPACES
                .get(usize::from(*id))
                .map_or("", |row| match class {
                    ConformanceClass::Strict => row.strict_uri.unwrap_or(row.uri),
                    ConformanceClass::Transitional => row.uri,
                }),
            Self::None => "",
            Self::Other(uri) => uri,
        }
    }
}

impl OfficeVersion {
    /// The release order: Office 2010 is 0, Microsoft 365 is 5.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Office2010 => 0,
            Self::Office2013 => 1,
            Self::Office2016 => 2,
            Self::Office2019 => 3,
            Self::Office2021 => 4,
            Self::Microsoft365 => 5,
        }
    }
}

impl QName {
    /// A name from its three parts, as they are.
    #[must_use]
    pub const fn new(prefix: String, local: String, namespace: Namespace) -> Self {
        Self {
            local,
            namespace,
            prefix,
        }
    }

    /// A name written as `prefix:local`, in the namespace `uri`. A known URI of either class
    /// gives `Namespace::Known`.
    #[must_use]
    pub fn prefixed(prefix: &str, local: &str, uri: &str) -> Self {
        Self {
            local: local.to_owned(),
            namespace: Namespace::from_uri(uri),
            prefix: prefix.to_owned(),
        }
    }

    /// A name with no prefix, in the namespace `uri`; an empty `uri` gives `Namespace::None`.
    #[must_use]
    pub fn unprefixed(local: &str, uri: &str) -> Self {
        Self::prefixed("", local, uri)
    }
}

/// The tag of a name in the namespace table: the namespace id (`None` for no namespace) and the
/// local name. Returns `None` for a namespace the table does not know.
#[must_use]
pub const fn tag_of(name: &QName) -> Option<(Option<NamespaceId>, &str)> {
    let local = name.local.as_str();
    match name.namespace {
        Namespace::Known(id) => Some((Some(id), local)),
        Namespace::None => Some((None, local)),
        Namespace::Other(_) => None,
    }
}
