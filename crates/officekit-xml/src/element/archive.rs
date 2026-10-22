//! The mirror rkyv archives `Node` through: the archived enum and resolver it
//! writes stay in this private module.

use rkyv::de::Pooling;
use rkyv::rancor::{Fallible, Source};
use rkyv::ser::{Allocator, Sharing, Writer};
use rkyv::validation::{ArchiveContext, SharedContext};
use rkyv::with::{ArchiveWith as _, DeserializeWith as _, SerializeWith as _};

#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(remote = super::Node)]
#[rkyv(serialize_bounds(__S: Allocator + Sharing + Writer, __S::Error: Source))]
#[rkyv(deserialize_bounds(__D: Pooling, __D::Error: Source))]
#[rkyv(bytecheck(bounds(__C: ArchiveContext + SharedContext, __C::Error: Source)))]
#[rkyv(attr(non_exhaustive))]
pub enum Node {
    CData(String),
    Comment(String),
    Element(#[rkyv(omit_bounds)] super::Element),
    ProcessingInstruction { content: String, target: String },
    Text(String),
}

impl From<Node> for super::Node {
    fn from(mirror: Node) -> Self {
        match mirror {
            Node::CData(text) => Self::CData(text),
            Node::Comment(text) => Self::Comment(text),
            Node::Element(element) => Self::Element(element),
            Node::ProcessingInstruction { content, target } => {
                Self::ProcessingInstruction { content, target }
            }
            Node::Text(text) => Self::Text(text),
        }
    }
}

impl rkyv::Archive for super::Node {
    type Archived = ArchivedNode;
    type Resolver = NodeResolver;

    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        Node::resolve_with(self, resolver, out);
    }
}

impl<S> rkyv::Serialize<S> for super::Node
where
    S: Fallible + Allocator + Sharing + Writer + ?Sized,
    S::Error: Source,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Node::serialize_with(self, serializer)
    }
}

impl<D> rkyv::Deserialize<super::Node, D> for ArchivedNode
where
    D: Fallible + Pooling + ?Sized,
    D::Error: Source,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<super::Node, D::Error> {
        Node::deserialize_with(self, deserializer)
    }
}
