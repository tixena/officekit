//! The mirror rkyv archives `MarkupCompatibility` through: the archived enum
//! and resolver it writes stay in this private module.

use rkyv::rancor::Fallible;
use rkyv::with::{ArchiveWith as _, DeserializeWith as _, SerializeWith as _};

use crate::name::OfficeVersion;

#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(remote = super::MarkupCompatibility)]
#[rkyv(attr(non_exhaustive))]
pub enum MarkupCompatibility {
    Keep,
    Process { target: OfficeVersion },
}

impl From<MarkupCompatibility> for super::MarkupCompatibility {
    fn from(mirror: MarkupCompatibility) -> Self {
        match mirror {
            MarkupCompatibility::Keep => Self::Keep,
            MarkupCompatibility::Process { target } => Self::Process { target },
        }
    }
}

impl rkyv::Archive for super::MarkupCompatibility {
    type Archived = ArchivedMarkupCompatibility;
    type Resolver = MarkupCompatibilityResolver;

    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        MarkupCompatibility::resolve_with(self, resolver, out);
    }
}

impl<S> rkyv::Serialize<S> for super::MarkupCompatibility
where
    S: Fallible + ?Sized,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        MarkupCompatibility::serialize_with(self, serializer)
    }
}

impl<D> rkyv::Deserialize<super::MarkupCompatibility, D> for ArchivedMarkupCompatibility
where
    D: Fallible + ?Sized,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<super::MarkupCompatibility, D::Error> {
        MarkupCompatibility::deserialize_with(self, deserializer)
    }
}
