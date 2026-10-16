# officekit

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0-7F6DF2.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange.svg)](https://rustup.rs)
[![Made by Tixena Labs](https://img.shields.io/badge/made_by-Tixena_Labs-7F6DF2.svg)](https://tixenalabs.com/)

> Read, write, and validate Office Open XML documents (`.docx`, `.xlsx`, `.pptx`) in Rust.

Officekit is a strongly typed model over the ECMA-376 schemas, on top of an
Open Packaging Conventions container. It ships as a library (`officekit-core`)
and a CLI (`officekit`).

**Status: scaffold.** The workspace, quality gate, and the OPC container are
in place. Content types, relationships, the element model, validation, and the
typed document classes are not ported yet.

## Layout

```
crates/officekit-core   library: OPC packaging, element model, validation, typed documents
crates/officekit        CLI binary
crates/xtask            dev tooling (`cargo xtask <subcommand>`), never installed
```

## Architecture

`officekit-core` follows the module layout of the
[Open XML SDK](https://github.com/dotnet/Open-XML-SDK), one module per namespace:

| Open XML SDK | officekit-core | State |
|---|---|---|
| `System.IO.Packaging` + `DocumentFormat.OpenXml.Packaging` | `packaging` | part names and the zip container; content types, relationships, and typed parts still to port |
| `DocumentFormat.OpenXml` (element model, simple types, reader/writer) | `xml` | planned |
| `DocumentFormat.OpenXml.Validation` | `validation` | planned |
| `DocumentFormat.OpenXml.Wordprocessing` + `WordprocessingDocument` | `wordprocessing` | planned |
| `DocumentFormat.OpenXml.Spreadsheet` + `SpreadsheetDocument` | `spreadsheet` | planned |
| `DocumentFormat.OpenXml.Presentation` + `PresentationDocument` | `presentation` | planned |

The generated schema namespaces (`Drawing`, `Math`, `VariantTypes`, and the
rest) will be produced by `cargo xtask generate-schema` from the SDK's schema
data, the same way the SDK generates its own classes.

### Design rules

- **Every byte in or out of the operating system passes through
  [`os-shim`](https://github.com/eburgos/os-shim).** Library functions that
  touch files take a `&S where S: os_shim::System + ?Sized`. The CLI hands in
  `RealSystem`; tests hand in `MemorySystem` and never touch disk.
- **The zip container is parsed from an in-memory buffer**, not a stream.
  Office files in the wild use data descriptors (the archive's central
  directory holds the sizes, the local headers do not), and a streaming reader
  fails on those. The bytes still arrive through `System::open`.
- **Part names are resolved without regard to ASCII case**, as ECMA-376 Part 2
  requires. `PartName` keeps the spelling it was given; `Package` files each
  part under the case-folded name.

## Building

```bash
just all      # clippy (restriction + pedantic + nursery denied), fmt check, build, tests
just lint
just test
just docs
```

## CLI

```bash
officekit parts report.docx          # one part name per line
officekit parts deck.pptx --json     # {"file": ..., "parts": [...]}
```

## License

MIT. See [LICENSE](LICENSE).
