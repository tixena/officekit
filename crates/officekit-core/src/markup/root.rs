// The root hooks every namespace's `Context` and `Visitor` extend (D12),
// brought into `markup` with `include!`.

/// The root of every context: the hooks for content the schemas do not
/// declare. Each namespace's `Context` extends this trait and no other.
pub trait Context {
    /// Asked for each generic element: an unknown element in a list's `Other`
    /// items, in a node's `unknown.children`, or in a wildcard position.
    /// `None` rebuilds the element from its transformed children.
    fn element(&mut self, _node: &xml::element::Element) -> Option<xml::element::Element> {
        None
    }

    /// Asked for each generic element written through, before `element`;
    /// `Written::Yes` means the hook wrote it.
    ///
    /// # Errors
    ///
    /// The sink's.
    fn write_element(
        &mut self,
        _node: &xml::element::Element,
        _out: &mut PartWriter<'_>,
    ) -> io::Result<Written> {
        Ok(Written::No)
    }
}

/// The root of every visitor: the hook for content the schemas do not
/// declare. Each namespace's `Visitor` extends this trait and no other.
pub trait Visitor {
    /// Shown each generic element; `Descend::Yes` walks into its children.
    fn element(&mut self, _node: &xml::element::Element) -> Descend {
        Descend::Yes
    }
}
