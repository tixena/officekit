//! The mirror rkyv archives `TargetMode` through: the archived enum and
//! resolver it writes stay in this private module.

use rkyv::rancor::Fallible;
use rkyv::with::{ArchiveWith as _, DeserializeWith as _, SerializeWith as _};

#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(remote = super::TargetMode)]
#[rkyv(attr(non_exhaustive))]
pub enum TargetMode {
    External,
    Internal,
}

impl From<TargetMode> for super::TargetMode {
    fn from(mirror: TargetMode) -> Self {
        match mirror {
            TargetMode::External => Self::External,
            TargetMode::Internal => Self::Internal,
        }
    }
}

impl rkyv::Archive for super::TargetMode {
    type Archived = ArchivedTargetMode;
    type Resolver = TargetModeResolver;

    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        TargetMode::resolve_with(self, resolver, out);
    }
}

impl<S> rkyv::Serialize<S> for super::TargetMode
where
    S: Fallible + ?Sized,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        TargetMode::serialize_with(self, serializer)
    }
}

impl<D> rkyv::Deserialize<super::TargetMode, D> for ArchivedTargetMode
where
    D: Fallible + ?Sized,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<super::TargetMode, D::Error> {
        TargetMode::deserialize_with(self, deserializer)
    }
}
