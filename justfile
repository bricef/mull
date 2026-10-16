# List recipes
default:
    @just --list

# Everything CI runs: formatting, lints, tests
check: fmt-check lint test

fmt-check:
    cargo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

test:
    cargo test

# Open a map in the terminal UI
run file:
    cargo run -- {{file}}

# Regenerate the README's key tables after changing src/keymap.rs
keys:
    cargo run -q -- --keys
