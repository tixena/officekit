# officekit

A library (`officekit-core`) and a CLI (`officekit`) for reading, editing,
writing, and validating Office Open XML documents (`.docx`, `.xlsx`, `.pptx`)
in either conformance class, Transitional or Strict.

## Layout

```
crates/officekit-core        library: element trees, file types
crates/officekit-packaging   library: the package, part names, content types, relationships, images
crates/officekit-validation  library: the validator and its generated tables
crates/officekit-xml         library: element tree, names, values, part reader and writer
crates/officekit             CLI binary
crates/xtask                 dev tooling (`cargo xtask <subcommand>`), never installed
crates/xtask/data/           the generator's hand-written inputs
```

Inside `officekit-core`, modules are organized by what each part of a document
is:

```
docx, xlsx, pptx   the file types: a package with its main part, class, and kind; saves; operations
markup             the typed tree, one module per namespace, with transforms and visitors
parts              one module per part type: content type, relationship types, load and store
stream             a transform written through as it is made
embedded           a document stored in a part of another
transitional       the Transitional class: read and write of every node (feature `transitional`)
strict             the Strict class: read and write of every node (feature `strict`)
```

## Rules

- **All OS access goes through `os_shim::System`.** No `std::fs`, `std::env`,
  or `std::process` calls anywhere in `crates/`. Library functions that touch
  files take `&S where S: System + ?Sized`; the CLI passes `RealSystem`, tests
  pass `MemorySystem`.
- **Zip parsing happens on an in-memory buffer**, never on a stream. Office
  files in the wild carry data descriptors, and a streaming reader cannot
  handle those. The bytes still arrive through `System::open`.
- **Quality gate is `just all`** (clippy with `restriction`, `pedantic`, and
  `nursery` denied; `cargo fmt --check`; build; tests). It runs the four
  feature builds: the workspace, `officekit-core` with all features, with no
  default features, and with `strict` alone. Nothing is done until it passes.
  Never add `#[allow]` / `#[expect]` to get past a lint; fix the code or raise
  it.
- **Generated files are never edited by hand**: everything under `markup/`,
  `transitional/`, `strict/`, and `parts/` of `officekit-core/src`,
  `crates/officekit-validation/src/generated/` (and `generated.rs`),
  `crates/officekit-xml/src/name/table.rs`, and
  `crates/officekit-packaging/src/relationship_type.rs`. Change the generator
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
