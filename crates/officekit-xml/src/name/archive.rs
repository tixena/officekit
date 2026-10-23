//! The mirrors rkyv archives `Namespace` and `OfficeVersion` through: the
//! archived enums and resolvers they write stay in this private module.

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

#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(remote = super::OfficeVersion)]
#[rkyv(attr(non_exhaustive))]
pub enum OfficeVersion {
    Microsoft365,
    Office2010,
    Office2013,
    Office2016,
    Office2019,
    Office2021,
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

impl From<OfficeVersion> for super::OfficeVersion {
    fn from(mirror: OfficeVersion) -> Self {
        match mirror {
            OfficeVersion::Microsoft365 => Self::Microsoft365,
            OfficeVersion::Office2010 => Self::Office2010,
            OfficeVersion::Office2013 => Self::Office2013,
            OfficeVersion::Office2016 => Self::Office2016,
            OfficeVersion::Office2019 => Self::Office2019,
            OfficeVersion::Office2021 => Self::Office2021,
        }
    }
}

impl rkyv::Archive for super::OfficeVersion {
    type Archived = ArchivedOfficeVersion;
    type Resolver = OfficeVersionResolver;

    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        OfficeVersion::resolve_with(self, resolver, out);
    }
}

impl<S> rkyv::Serialize<S> for super::OfficeVersion
where
    S: Fallible + ?Sized,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        OfficeVersion::serialize_with(self, serializer)
    }
}

impl<D> rkyv::Deserialize<super::OfficeVersion, D> for ArchivedOfficeVersion
where
    D: Fallible + ?Sized,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<super::OfficeVersion, D::Error> {
        OfficeVersion::deserialize_with(self, deserializer)
    }
}
