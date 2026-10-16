# officekit

A library (`officekit-core`) and a CLI (`officekit`) for reading, writing, and
validating Office Open XML documents (`.docx`, `.xlsx`, `.pptx`). The module
layout follows the Open XML SDK's namespaces.

## Layout

```
crates/officekit-core   library: OPC packaging, element model, validation, typed documents
crates/officekit        CLI binary
crates/xtask            dev tooling (`cargo xtask <subcommand>`), never installed
```

Inside `officekit-core`, modules follow the SDK's namespaces: `packaging`
(`DocumentFormat.OpenXml.Packaging` + `System.IO.Packaging`), `xml` (the root
`DocumentFormat.OpenXml` element model), `validation`, and one module per
document kind (`wordprocessing`, `spreadsheet`, `presentation`).

## Rules

- **All OS access goes through `os_shim::System`.** No `std::fs`, `std::env`,
  or `std::process` calls anywhere in `crates/`. Library functions that touch
  files take `&S where S: System + ?Sized`; the CLI passes `RealSystem`, tests
  pass `MemorySystem`.
- **Zip parsing happens on an in-memory buffer**, never on a stream. Office
  files in the wild carry data descriptors, and a streaming reader cannot
  handle those. The bytes still arrive through `System::open`.
- **Quality gate is `just all`** (clippy with `restriction`, `pedantic`, and
  `nursery` denied; `cargo fmt --check`; build; tests). Nothing is done until
  it passes. Never add `#[allow]` / `#[expect]` to get past a lint; fix the
  code or raise it.
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
