######################
# Development
######################

# Default recipe - show available commands
default:
    @just --list

# Run all tests
# Uses nextest for faster parallel execution if available, falls back to cargo test
test:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v cargo-nextest &> /dev/null; then
        echo "Running tests with nextest (parallel)..."
        cargo nextest run
    else
        echo "Running tests with cargo test..."
        cargo test
    fi

# Check clippy
check-clippy:
    cargo clippy --all-targets -- -D warnings

# Fix clippy issues automatically where possible
fix-clippy:
    cargo clippy --all-targets --fix --allow-dirty --allow-staged

# Check formatting
check-fmt:
    cargo fmt -- --check

# Apply formatting
fmt:
    cargo fmt

# Check all (fmt + clippy + test)
check: check-fmt check-clippy test

# Pre-commit checks: quiet mode with minimal output
precommit:
    @just _run-quiet "check-fmt"    "fmt"
    @just _run-quiet "check-clippy" "clippy"
    @just _run-quiet "test"         "tests"
    @echo "PRECOMMIT PASSED"

# Pre-commit checks with verbose output
precommit-verbose: check

######################
# Build
######################

# Build release binary
build:
    cargo build --release
    @echo "Binary: target/release/wn-tui"

# Build and run
run: build
    ./target/release/wn-tui

######################
# Maintenance
######################

# Check for outdated dependencies
outdated:
    cargo outdated --root-deps-only

# Update dependencies
update:
    cargo update

# Audit dependencies for security vulnerabilities
audit:
    cargo audit

# Clean build artifacts
clean:
    cargo clean

######################
# Helper Recipes
######################

# Run a recipe quietly, showing only name and pass/fail status (internal use)
[private]
_run-quiet recipe label:
    #!/usr/bin/env bash
    TMPFILE=$(mktemp)
    trap 'rm -f "$TMPFILE"' EXIT
    printf "%-25s" "{{label}}..."
    if just {{recipe}} > "$TMPFILE" 2>&1; then
        echo "✓"
    else
        echo "✗"
        echo ""
        cat "$TMPFILE"
        exit 1
    fi
