# Default: list recipes.
default:
    @just --list

# Lint Rust: clippy across the whole workspace, then fmt check.
# `--workspace` because `default-members` excludes `crates/xtask`: without
# it the dev tooling is neither linted nor tested by the gate.
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all --check

# Format Rust.
fmt:
    cargo fmt --all

# Build every crate.
build:
    cargo build --workspace

# Run the test suite.
test:
    cargo test --workspace

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

# Full pipeline (standardized `all` entry point across tixena repos).
all: lint build test
