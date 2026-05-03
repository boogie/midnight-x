#!/usr/bin/env bash
# Build (release) and launch `mx`. Forwards any extra args, e.g.:
#   scripts/run.sh --version
#   scripts/run.sh --config /path/to/config.toml
set -euo pipefail

# Locate repo root regardless of where the script is invoked from.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$REPO_ROOT"

# brew's `rustup` keg is keg-only; prepend it so cargo is on PATH.
if [[ -d /opt/homebrew/opt/rustup/bin ]]; then
    export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not on PATH; install rust via 'brew install rustup && rustup default stable'" >&2
    exit 127
fi

# Build first so compile diagnostics are obvious; then exec the binary
# directly so its exit code (and signals like Ctrl-C) are what the caller
# sees, not cargo's wrapper.
cargo build -p mx --release

exec target/release/mx "$@"
