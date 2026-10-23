# Default: list recipes.
default:
    @just --list

# Lint Rust: clippy across the whole workspace, then officekit-core and
# officekit-parts in every feature combination, then fmt check, and rustfmt
# over every test file, which reaches the ones `include!` hides. `--workspace`
# because `default-members` excludes `crates/xtask`. officekit-markup-core is
# checked first and alone: its compile takes most of the memory cap, so no
# other compile runs beside it.
lint:
    cargo clippy -p officekit-markup-core --all-targets -- -D warnings
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p officekit-core --all-targets --all-features -- -D warnings
    cargo clippy -p officekit-core --all-targets --no-default-features -- -D warnings
    cargo clippy -p officekit-core --all-targets --no-default-features --features strict -- -D warnings
    cargo clippy -p officekit-parts --all-targets --all-features -- -D warnings
    cargo clippy -p officekit-parts --all-targets --no-default-features -- -D warnings
    cargo clippy -p officekit-parts --all-targets --no-default-features --features strict -- -D warnings
    cargo clippy -p officekit --all-targets --no-default-features -- -D warnings
    cargo fmt --all --check
    find crates -name tests.rs -print0 | xargs -0 rustfmt --edition 2024 --check

# Format Rust.
fmt:
    cargo fmt --all

# Build every crate, officekit-markup-core first and alone.
build:
    cargo build -p officekit-markup-core
    cargo build --workspace

# Run the test suite, officekit-core's and officekit-parts' with every class on,
# officekit-core's with Strict alone, and the CLI's without the binary form. officekit-markup-core builds first and
# alone. The next line builds the libraries, binaries and examples with the
# dev-dependencies' features, as the test run resolves them, so `cargo test
# --workspace` compiles test targets only and no generated crate's library
# compiles beside its test binary.
test:
    cargo build -p officekit-markup-core
    cargo build --workspace --lib --bins --examples
    cargo test --workspace
    cargo test -p officekit-core --all-features
    cargo test -p officekit-parts --all-features
    cargo test -p officekit-core --no-default-features --features strict
    cargo test -p officekit --no-default-features

# The gate for each change: fmt, then clippy, a build and a test run of the
# whole workspace with every feature on, each over one feature set. `just full`
# adds the builds with fewer features, and the audit.
check:
    cargo fmt --all --check
    find crates -name tests.rs -print0 | xargs -0 rustfmt --edition 2024 --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    # One job: officekit-markup-core peaks at 7 GB, and officekit-markup-ext
    # compiling beside it passes the 9 GB cap.
    cargo build --workspace --all-targets --all-features -j 1
    cargo test --workspace --all-features

# Generate code coverage report (requires cargo-llvm-cov)
test-coverage:
    @echo "Generating code coverage report..."
    cargo llvm-cov --workspace

# Generate code coverage HTML report (requires cargo-llvm-cov)
test-coverage-html:
    @echo "Generating code coverage HTML report..."
    cargo llvm-cov --workspace --html
    @echo "Coverage report generated in target/llvm-cov/html/"

# Audit dependencies for security advisories (requires cargo-audit).
audit:
    cargo audit

# Generate API documentation.
docs:
    cargo doc --workspace --no-deps

# Validate every Office file in a folder with SDK 3.5.1 and print its errors as
# JSON. Not part of `all`: it needs .NET 10 and NuGet.
oracle folder:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    files=("{{folder}}"/*.docx "{{folder}}"/*.xlsx "{{folder}}"/*.pptx)
    if [ "${#files[@]}" -eq 0 ]; then echo "no .docx, .xlsx, or .pptx file in {{folder}}" >&2; exit 2; fi
    dotnet run --project oracle -c Release -- "${files[@]}"

# Compare officekit's validation with SDK 3.5.1 on every Office file in DIR (needs .NET 10). Not part of `all`.
parity DIR kinds="schema,markup-compatibility":
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    files=("{{DIR}}"/*.docx "{{DIR}}"/*.xlsx "{{DIR}}"/*.pptx)
    if [ "${#files[@]}" -eq 0 ]; then echo "no .docx, .xlsx, or .pptx file in {{DIR}}" >&2; exit 2; fi
    rm -rf target/parity && mkdir -p target/parity/officekit
    cargo build -q -p officekit
    dotnet run --project oracle -c Release -- "${files[@]}" > target/parity/oracle.json
    for f in "${files[@]}"; do for t in 2010 2013 2016 2019 2021 365; do
      target/debug/officekit validate --json --max-errors 0 --target "$t" "$f" > "target/parity/officekit/$(basename "$f").$t.json" \
      || [ $? -eq 1 ] || exit 2
    done; done
    cargo xtask parity target/parity/oracle.json target/parity/officekit --kinds {{kinds}}

# Read the semantic constraints SDK 3.5.1 registers from its package, by
# reflection, into inputs/sdk-3.5.1-semantic.json (git-ignored, never
# committed), and record its SHA-256 in inputs/SHA256SUMS. The generator
# checks its rows against it and takes from it any registration no Schematron
# rule of the data gives. Not part of `all`: it needs .NET 10 and NuGet.
semantic-inputs:
    #!/usr/bin/env bash
    set -euo pipefail
    dump=inputs/sdk-3.5.1-semantic.json
    dotnet run --project oracle/semantic-dump -c Release > "$dump.part"
    mv -f "$dump.part" "$dump"
    { awk -v dump="$dump" '$2 != dump' inputs/SHA256SUMS; sha256sum "$dump"; } \
        | LC_ALL=C sort -k 2 > inputs/SHA256SUMS.part
    mv -f inputs/SHA256SUMS.part inputs/SHA256SUMS

# Download the generator's inputs into inputs/ (git-ignored, never committed).
# Files already present are not downloaded again.
#
# The 18 namespaces the SDK data leaves empty were searched for in the latest
# published PDF of every Office standards document on
# https://learn.microsoft.com/en-us/openspecs/office_standards/ ([MS-CUSTOMUI],
# [MS-CUSTOMUI2], [MS-DOCX], [MS-ODRAWXML], [MS-OE376], [MS-OEXTXML],
# [MS-OFFMACRO], [MS-OFFMACRO2], [MS-OI29500], [MS-OINTXML], [MS-OREACTXML],
# [MS-OTASKXML], [MS-OWEXML], [MS-PPTX], [MS-XLSX]). The 13 found are fetched
# from their Appendix A page; the 5 no document declares stay generic.
inputs:
    #!/usr/bin/env bash
    set -euo pipefail
    for tool in curl unzip git pdftotext sha256sum; do
        command -v "$tool" >/dev/null || { echo "inputs: $tool is not installed" >&2; exit 1; }
    done
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    mkdir -p inputs/ecma-376/{strict,transitional,opc} inputs/open-specifications

    fetch() { # fetch <url> <destination>
        [ -s "$2" ] && return 0
        curl -fL --retry 3 -o "$2.part" "$1"
        mv -f "$2.part" "$2"
    }

    # ECMA-376, 5th edition: the four parts.
    base=https://ecma-international.org/wp-content/uploads
    ecma=inputs/ecma-376
    fetch "$base/ECMA-376-1_5th_edition_december_2016.zip" $ecma/part1.zip
    fetch "$base/ECMA-376-2_5th_edition_december_2021.zip" $ecma/part2.zip
    fetch "$base/ECMA-376-3_5th_edition_december_2015.zip" $ecma/part3.zip
    fetch "$base/ECMA-376-4_5th_edition_december_2016.zip" $ecma/part4.zip

    # The Part 1 and Part 4 PDFs, and their text.
    [ -s $ecma/part1.pdf ] || unzip -p $ecma/part1.zip \
        "Ecma Office Open XML Part 1 - Fundamentals And Markup Language Reference.pdf" > $ecma/part1.pdf
    [ -s $ecma/part4.pdf ] || unzip -p $ecma/part4.zip \
        "Ecma Office Open XML Part 4 - Transitional Migration Features.pdf" > $ecma/part4.pdf
    for part in part1 part4; do
        [ -s $ecma/$part.txt ] || pdftotext -layout $ecma/$part.pdf $ecma/$part.txt
    done

    # The schemas: Strict from Part 1, Transitional from Part 4, OPC from Part 2.
    schemas() { # schemas <part zip> <inner zip> <destination folder>
        [ -n "$(ls -A "$3")" ] && return 0
        unzip -p "$1" "$2" > "$work/$2"
        unzip -oq "$work/$2" -d "$3"
    }
    schemas $ecma/part1.zip OfficeOpenXML-XMLSchema-Strict.zip $ecma/strict
    schemas $ecma/part4.zip OfficeOpenXML-XMLSchema-Transitional.zip $ecma/transitional
    schemas $ecma/part2.zip OpenPackagingConventions-XMLSchema.zip $ecma/opc

    # The Open XML SDK's data/ folder, a sparse checkout at the tag.
    sdk=inputs/open-xml-sdk
    sdk_commit=3139fdfd27414548a41555f7848d5728f6e71a42
    if [ ! -d $sdk/.git ]; then
        git -C inputs clone --filter=blob:none --no-checkout --depth 1 --branch v3.5.1 \
            https://github.com/dotnet/Open-XML-SDK open-xml-sdk
        git -C $sdk sparse-checkout set data
        git -C $sdk checkout
    fi
    test "$(git -C $sdk rev-parse HEAD)" = "$sdk_commit"

    # The Open Specifications XSDs: prefix|document|section|namespace|page.
    # The page is the Appendix A section of the document, which prints the
    # schema in a <pre> element. A document of "-" means no document declares
    # the namespace, so it stays generic.
    osdir=inputs/open-specifications
    pages=https://learn.microsoft.com/en-us/openspecs/office_standards
    : > $osdir/SOURCES.part
    while IFS='|' read -r prefix document section namespace page; do
        if [ "$document" = "-" ]; then
            printf '%s\t%s\t-\t-\tno source\n' "$prefix" "$namespace" >> $osdir/SOURCES.part
            continue
        fi
        printf '%s\t%s\t[%s]\t%s\t%s.xsd\n' "$prefix" "$namespace" "$document" "$section" "$prefix" >> $osdir/SOURCES.part
        if [ ! -s "$osdir/$prefix.xsd" ]; then
            doc=$(printf '%s' "$document" | tr '[:upper:]' '[:lower:]')
            fetch "$pages/$doc/$page" "$work/$prefix.html"
            sed -n '/<pre>/,/<\/pre>/p' "$work/$prefix.html" \
                | sed -e 's/<[^>]*>//g' -e 's/&nbsp;/ /g' -e 's/&lt;/</g' -e 's/&gt;/>/g' \
                      -e 's/&quot;/"/g' -e 's/&amp;/\&/g' > "$osdir/$prefix.xsd"
            [ -s "$osdir/$prefix.xsd" ] || { echo "inputs: no schema in $document $section" >&2; rm -f "$osdir/$prefix.xsd"; exit 1; }
        fi
    done <<'TABLE'
    aink|MS-ODRAWXML|5.28|http://schemas.microsoft.com/office/drawing/2016/ink|9fb2d49d-0881-4e7d-8b38-95373d76302c
    cr|MS-OREACTXML|5.1|http://schemas.microsoft.com/office/comments/2020/reactions|e0f0814f-387d-40bd-adf1-10aa522d3989
    md|-|-|http://schemas.microsoft.com/office/office/2011/9/metroDictionary|-
    pav|-|-|http://schemas.microsoft.com/office/2007/6/19/audiovideo|-
    pc226|MS-PPTX|5.18|http://schemas.microsoft.com/office/powerpoint/2022/06/main/command|7501f644-0980-4b9f-ad3c-b827d9f74501
    pj15|-|-|http://schemas.microsoft.com/projectml/2012/main|-
    w16sdtdh|MS-DOCX|5.7|http://schemas.microsoft.com/office/word/2020/wordml/sdtdatahash|c865ba38-51df-45bc-91ae-f6beaffeb264
    w16sdtfl|MS-DOCX|5.9|http://schemas.microsoft.com/office/word/2024/wordml/sdtformatlock|14bc8126-7c6e-41a8-b042-c199036e2339
    x16r2|MS-XLSX|5.11|http://schemas.microsoft.com/office/spreadsheetml/2015/02/main|e29f5a54-ff71-47e9-9183-440b65eb27ad
    x16r3|MS-XLSX|5.24|http://schemas.microsoft.com/office/spreadsheetml/2018/08/main|b38f8644-d3e6-4d20-889d-56a9df09cad8
    xlPr|-|-|http://schemas.microsoft.com/office/spreadsheetml/2016/01/main|-
    xr10|MS-XLSX|5.12|http://schemas.microsoft.com/office/spreadsheetml/2016/revision10|8a818a7f-92d0-4390-9855-1a484d8f9347
    xr16|-|-|http://schemas.microsoft.com/office/spreadsheetml/2017/revision16|-
    xr2|MS-XLSX|5.16|http://schemas.microsoft.com/office/spreadsheetml/2015/revision2|7b729443-d0cd-434e-8d8c-b228f701a55b
    xr3|MS-XLSX|5.17|http://schemas.microsoft.com/office/spreadsheetml/2016/revision3|1c6fdaed-8a40-4086-be07-0135b10b8f90
    xr5|MS-XLSX|5.18|http://schemas.microsoft.com/office/spreadsheetml/2016/revision5|83ec1155-c05a-4c5e-84b5-844b18358690
    xr6|MS-XLSX|5.14|http://schemas.microsoft.com/office/spreadsheetml/2016/revision6|e47e052c-40d0-43cd-add4-73a220966b69
    xr9|MS-XLSX|5.13|http://schemas.microsoft.com/office/spreadsheetml/2016/revision9|1df7cd78-d8bc-4ec4-8952-1b61beeaee76
    TABLE
    mv -f $osdir/SOURCES.part $osdir/SOURCES

    # The tools and the checksums of everything above.
    version=$(pdftotext -v 2>&1)
    printf '%s\nopen-xml-sdk %s (v3.5.1)\n' "${version%%$'\n'*}" "$sdk_commit" > inputs/TOOLS
    find inputs/ -type f ! -path 'inputs/SHA256SUMS' ! -name '*.part' \
        \( -path 'inputs/open-xml-sdk/data/*' -o ! -path 'inputs/open-xml-sdk/*' \) -print0 \
        | LC_ALL=C sort -z | xargs -0 sha256sum > inputs/SHA256SUMS.part
    mv -f inputs/SHA256SUMS.part inputs/SHA256SUMS

# Full pipeline (standardized `all` entry point across tixena repos).
all: lint build test

# The full gate, every feature build and the audit; slower than `just check`,
# run every so often.
full: all audit
