# officekit

A library (`officekit`) and a CLI (`officekit-cli`) for reading, editing,
writing, and validating Office Open XML documents (`.docx`, `.xlsx`, `.pptx`)
in either conformance class, Transitional or Strict.

## Layout

```
crates/officekit               library: element trees, file types; the part modules (`parts`), the Transitional class (`transitional`, feature `transitional`), and the validator (`validation`)
crates/officekit-markdown      library: a Word file from markdown and markdown from a Word file, with the `DocxMarkdown` and `OfficeFileMarkdown` traits
crates/officekit-markup        library: the typed tree, officekit_markup::<namespace>: the namespaces outside base and core, the root hook traits, every namespace's hook traits, and base's and core's namespaces re-exported
crates/officekit-markup-base   library: the runtime and the namespaces the cycle depends on
crates/officekit-markup-core   library: the namespaces that reference each other in a cycle
crates/officekit-strict        library: the Strict class: read and write of every node (feature `strict` of officekit); the typed tree's transforms, visitors and write-through (`traversal`)
crates/officekit-xml           library: element tree, names, values, part reader and writer; the package, part names, content types, relationships, images (`packaging`)
crates/officekit-cli           CLI binary
crates/xtask                   dev tooling (`cargo xtask <subcommand>`), never installed
crates/xtask/data/             the generator's hand-written inputs
```

Inside `officekit`, modules are organized by what each part of a document
is:

```
docx, xlsx, pptx   the file types: a package with its main part, class, and kind; saves; operations
office_file        a .docx, .xlsx or .pptx opened from its bytes without knowing which
binary             officekit's binary form: a file's whole model as an rkyv archive, and back
embedded           a document stored in a part of another
compact            the merge of neighbouring runs that render the same
folder             a document as a folder of its parts, and back
parts              one module per part type: content type, relationship types, load and store
transitional       the Transitional class: read and write of every node, and the conversion of date cells (feature `transitional`)
validation         the validator and its generated tables
```

## Rules

- **All OS access goes through `os_shim::System`.** No `std::fs`, `std::env`,
  or `std::process` calls anywhere in `crates/`. Library functions that touch
  files take `&S where S: System + ?Sized`; the CLI passes `RealSystem`, tests
  pass `MemorySystem`. Those functions are `async`, because only the caller
  knows what its system's futures wait on. Everything with a system of its own
  stays synchronous and runs it with `officekit_xml::ready::ready_now`: `Package`
  over its memory filesystem, the CLI, `xtask`, the examples and the tests.
- **Zip parsing happens on an in-memory buffer**, never on a stream. Office
  files in the wild carry data descriptors, and a streaming reader cannot
  handle those. The bytes still arrive through `System::read`.
- **Quality gate is `just all`** (clippy with `restriction`, `pedantic`, and
  `nursery` denied; `cargo fmt --check`; build; tests). It runs the feature
  builds: the workspace, then `officekit` with all features, with no default
  features, and with `strict` alone, and `officekit-markdown` with all features.
  Nothing is done until it passes. Never add `#[allow]` / `#[expect]` to get
  past a lint; fix the code or raise it.
- **Generated files are never edited by hand**: everything under
  `crates/officekit-markup-base/src` but `lib.rs`, `class_rules.rs`, and the
  unit tests; everything under `crates/officekit-markup-core/src` and
  `crates/officekit-markup/src` but `lib.rs` and the unit tests;
  everything under `crates/officekit-strict/src` but `lib.rs`, `traversal.rs`
  and the unit tests; everything under `crates/officekit/src/transitional` but
  `dates.rs` and the unit tests; `crates/officekit/src/parts.rs` and everything
  under `crates/officekit/src/parts` but `part_root.rs` and the unit tests;
  `crates/officekit/src/validation/generated/` (and `generated.rs`);
  `crates/officekit-xml/src/name/table.rs`; and
  `crates/officekit-xml/src/packaging/relationship_type.rs`. Change the generator
  in `crates/xtask` (its hand-written inputs live in `crates/xtask/data/`) and
  run `cargo xtask generate-schema`.
- **Module files, not `mod.rs`**: `foo.rs` next to `foo/`. Unit tests live in
  `foo/tests.rs` behind `#[cfg(test)] mod tests;`. Integration tests are
  `tests/<name>.rs` stubs that `#[path]`-include `tests/<name>/tests.rs`.
- Prefer `core::` and `alloc::` over `std::` where the item exists there
  (clippy `std_instead_of_core` / `std_instead_of_alloc` are on).
- Import traits used only for their methods as `use Trait as _;`.

## Non-Interactive Shell Commands

`cp`, `mv`, and `rm` may be aliased with `-i` on this machine. Always pass
`-f` (`cp -f`, `mv -f`, `rm -f`, `rm -rf`) so an agent never hangs on a
confirmation prompt.
