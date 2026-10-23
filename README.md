# officekit

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0-7F6DF2.svg)](Cargo.toml)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange.svg)](https://rustup.rs)
[![Made by Tixena Labs](https://img.shields.io/badge/made_by-Tixena_Labs-7F6DF2.svg)](https://tixenalabs.com/)

> Read, write, and validate Office Open XML documents (`.docx`, `.xlsx`, `.pptx`) in Rust.

Officekit reads, edits, writes, and validates Office Open XML files in either
conformance class of ECMA-376, Transitional or Strict. It ships as a library
(`officekit-core`) and a CLI (`officekit`). The typed tree and the part modules
are generated from the ECMA-376 schemas and the extension schema data of the
[Open XML SDK](https://github.com/dotnet/Open-XML-SDK); the validator is
checked against SDK 3.5.1, which serves as its oracle.

## Status

Built and tested:

- the Open Packaging Conventions container: part names, content types,
  relationships, and the zip archive, parsed from an in-memory buffer;
- the generic element tree and the typed tree, one module per namespace, with
  transforms, visitors, and write-through;
- the Transitional and Strict readers and writers of every node;
- one module per part type, with its content type and its relationship type in
  both classes;
- the `.docx`, `.xlsx`, and `.pptx` file types, with their saves in either
  class and the operations over the typed tree;
- streaming a part and a package;
- the validator: schema, markup compatibility, package, and semantic checks
  against an Office version, 2010 to 365;
- the CLI: `parts`, `cat`, `validate`, `down`, `up`, and `format`.

## Layout

```
crates/officekit-core          library: element trees, file types
crates/officekit-markup        library: the typed tree, one module per namespace, with readers, writers and hooks
crates/officekit-packaging     library: the package, part names, content types, relationships, images
crates/officekit-parts         library: one module per part type: content type, relationship types, load and store
crates/officekit-strict        library: the Strict class: read and write of every node
crates/officekit-transitional  library: the Transitional class: read and write of every node, date-cell conversion
crates/officekit-traversal     library: the typed tree's transforms, visitors and write-through
crates/officekit-validation    library: the validator and its generated tables
crates/officekit-xml           library: element tree, names, values, part reader and writer
crates/officekit               CLI binary
crates/xtask                   dev tooling (`cargo xtask <subcommand>`), never installed
```

## Modules

`officekit-core` is organized by what each part of a document is:

| Module | Covers | Holds |
|---|---|---|
| `docx`, `xlsx`, `pptx` | The three file types | A package with its main part, its class, and its kind; the saves in either class; the operations over the typed tree |
| `embedded` | A document stored in a part of another | A `.docx`, `.xlsx`, or `.pptx` inside a package |
| `compact` | Run compaction | The merge of neighbouring runs that render the same |
| `folder` | Expanded documents | A `.docx` as a folder of its parts, and back |
| `markdown` | Markdown documents (feature `markdown`) | A Word document from an archie markdown document, and back |

`officekit-parts` holds one generated module per part type of ECMA-376 Part 1
and of Microsoft's extensions: its content type, its relationship type in both
classes' spelling, and the load and store of its root.

## Features

`officekit-core` has four features and `officekit-parts` three; the two class
features of `officekit-core` turn on those of `officekit-parts`, and so does
`binary-format`. Each class feature turns on its class crate as an optional
dependency:

| Feature | Default | Enables |
|---|---|---|
| `transitional` | yes | The `officekit-transitional` crate (ECMA-376 Part 1 with the migration features of Part 4): the generated read and write of every node of the typed tree as Transitional, and the conversion of date cells to number cells before a Transitional save |
| `strict` | no | The `officekit-strict` crate (ECMA-376 Part 1): the generated read and write of every node as Strict, and the generated rules that rename, unpack, and refuse what Strict spells differently |
| `binary-format` | no | The `binary` module: officekit's binary form, a file's whole model as an rkyv archive, and back. It turns on rkyv and the archive of every type of the typed tree, the package and the part modules; without it none of that compiles |
| `markdown` | no | The `markdown` module: a Word document from an archie markdown document, and a Word document written as markdown, its pictures in a folder beside the file or inside the markdown as data URIs. Turns on `transitional` |

Markdown to Word and back changes five things, and nothing else:

- A code block loses its language and arguments.
- A wiki link `[[target|text]]` comes back as `[text](target)`.
- A loose list comes back tight, unless one of its items holds two paragraphs one after the other.
- A simple table, or a grid table with exactly one head row whose cells each hold one line, comes back as a pipe table.
- A paragraph comes back as one line.

Pictures also come back under the media folder (`report_media/image1.png`), and a WebP as a PNG.

```bash
cargo build -p officekit-core                                             # transitional
cargo build -p officekit-core --no-default-features --features strict    # strict only
cargo build -p officekit-core --all-features                              # both
cargo build -p officekit-parts --no-default-features --features strict   # the part modules, strict only
```

## Design rules

- **Every byte in or out of the operating system passes through
  [`os-shim`](https://github.com/tixena/os-shim).** Library functions that
  touch files take a `&S where S: os_shim::System + ?Sized`. The CLI hands in
  `RealSystem`; tests hand in `MemorySystem` and never touch disk.
- **The zip container is parsed from an in-memory buffer**, not a stream.
  Office files in the wild use data descriptors (the archive's central
  directory holds the sizes, the local headers do not), and a streaming reader
  fails on those. The bytes still arrive through `System::open`.
- **Part names are resolved without regard to ASCII case**, as ECMA-376 Part 2
  requires. `PartName` keeps the spelling it was given; `Package` files each
  part under the case-folded name.

## CLI

```bash
officekit parts report.docx                  # one part name per line
officekit parts deck.pptx --json             # {"file": ..., "parts": [...]}
officekit cat report.docx /word/document.xml # the bytes of one part on standard output
officekit validate report.docx               # one line per error
officekit validate book.xlsx --json --target 2016 --max-errors 50
officekit down docs/                         # expand every .docx under docs/ into folders
officekit up docs/                           # zip the folders back into .docx files
officekit format docs/                       # re-indent the XML parts of the folders
```

`get -f binary` and `save` need the CLI's `binary-format` feature, on by default.
Built with `--no-default-features`, both exit 2 with an error that says so.

`validate` takes:

| Option | Meaning |
|---|---|
| `--target <2010\|2013\|2016\|2019\|2021\|365>` | The Office version to check against; default `365` |
| `--max-errors <N>` | Stop after N errors; `0` means no limit; default `1000` |
| `--json` | Output one JSON array of records |

Exit status of `validate`: `0` when the file has no error, `1` when it has at
least one, `2` when the check cannot run (the file cannot be read, for
example); the reason goes to standard error.

## Building

```bash
just check    # the gate for each change: fmt, then clippy, build and tests of the workspace with every feature on
just full     # the full gate: lint, build, test, audit
just all      # lint, build, test
just lint     # clippy (restriction + pedantic + nursery denied) on the workspace, then officekit-core and officekit-parts in each feature build; fmt check; rustfmt check of every tests.rs
just build
just test     # the workspace, then officekit-core and officekit-parts with all features, then officekit-core with strict alone
just docs     # cargo doc --workspace --no-deps
just audit    # cargo audit
just bpublish # build the CLI for this machine's target and add it to the GitHub release of the version
```

`just all` never needs .NET or the generator's inputs.

## The generator and its inputs

`cargo xtask generate-schema` writes the typed tree into
`crates/officekit-markup-base/src`, `crates/officekit-markup-core/src` and
`crates/officekit-markup-ext/src`, and the facade's `lib.rs` into
`crates/officekit-markup/src`, its traversals into
`crates/officekit-traversal/src`, the conformance classes into
`crates/officekit-transitional/src` and `crates/officekit-strict/src`, the
part modules of `officekit-parts` (`crates/officekit-parts/src`), the
validation tables of `officekit-validation`
(`crates/officekit-validation/src/generated/`), the name table of
`officekit-xml` (`crates/officekit-xml/src/name/table.rs`), and the
relationship types of `officekit-packaging`
(`crates/officekit-packaging/src/relationship_type.rs`).
Those files are generated and are never edited by hand; change the generator
in `crates/xtask` and run it again.
The generator's hand-written inputs live in `crates/xtask/data/`.

Its downloaded inputs go to `inputs/`, which is git-ignored:

```bash
just inputs            # download ECMA-376 5th edition, the SDK's data/ folder, the Open Specifications schemas
just semantic-inputs   # read SDK 3.5.1's semantic constraints by reflection (needs .NET 10)
cargo xtask generate-schema
```

## Parity with SDK 3.5.1

```bash
just parity DIR
```

validates every `.docx`, `.xlsx`, and `.pptx` in `DIR` with SDK 3.5.1 and with
`officekit validate` for each Office version, and prints every error the SDK
reports that officekit lacks, with a count per kind. It needs .NET 10 and
NuGet, and it is not part of `just all`. `just oracle FOLDER` prints the SDK's
errors for a folder as JSON.

## License

MIT, Copyright (c) 2026 Tixena Labs. See [LICENSE](LICENSE).
