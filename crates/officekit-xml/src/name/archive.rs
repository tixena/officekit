//! The mirror rkyv archives `Namespace` through: the archived enum and
//! resolver it writes stay in this private module.

use alloc::sync::Arc;

use rkyv::de::Pooling;
use rkyv::rancor::{Fallible, Source};
use rkyv::ser::{Sharing, Writer};
use rkyv::with::{ArchiveWith as _, DeserializeWith as _, SerializeWith as _};

use super::NamespaceId;

#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(remote = super::Namespace)]
#[rkyv(attr(non_exhaustive))]
pub enum Namespace {
    Known(NamespaceId),
    None,
    Other(Arc<str>),
}

impl From<Namespace> for super::Namespace {
    fn from(mirror: Namespace) -> Self {
        match mirror {
            Namespace::Known(id) => Self::Known(id),
            Namespace::None => Self::None,
            Namespace::Other(uri) => Self::Other(uri),
        }
    }
}

impl rkyv::Archive for super::Namespace {
    type Archived = ArchivedNamespace;
    type Resolver = NamespaceResolver;

    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        Namespace::resolve_with(self, resolver, out);
    }
}

impl<S> rkyv::Serialize<S> for super::Namespace
where
    S: Fallible + Sharing + Writer + ?Sized,
    S::Error: Source,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Namespace::serialize_with(self, serializer)
    }
}

impl<D> rkyv::Deserialize<super::Namespace, D> for ArchivedNamespace
where
    D: Fallible + Pooling + ?Sized,
    D::Error: Source,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<super::Namespace, D::Error> {
        Namespace::deserialize_with(self, deserializer)
    }
}
