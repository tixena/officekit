# Default: list recipes.
default:
    @just --list

# Lint Rust: clippy across the whole workspace, then officekit-core in every
# feature combination, then fmt check. `--workspace` because `default-members`
# excludes `crates/xtask`.
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p officekit-core --all-targets --all-features -- -D warnings
    cargo clippy -p officekit-core --all-targets --no-default-features -- -D warnings
    cargo clippy -p officekit-core --all-targets --no-default-features --features strict -- -D warnings
    cargo fmt --all --check

# Format Rust.
fmt:
    cargo fmt --all

# Build every crate.
build:
    cargo build --workspace

# Run the test suite, and officekit-core's with every class on.
test:
    cargo test --workspace
    cargo test -p officekit-core --all-features

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
    dotnet run --project oracle -c Release -- {{folder}}/*.docx {{folder}}/*.xlsx {{folder}}/*.pptx

# Full pipeline (standardized `all` entry point across tixena repos).
all: lint build test
