#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install Rust: https://rustup.rs" >&2
    exit 1
fi

echo "Installing git-task + ght (cargo install --locked --path .)..."
cargo install --locked --path .

CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
echo
if command -v git-task >/dev/null 2>&1; then
    echo "Done. Try:"
    echo "  git task            # banner, confirms git-task is on PATH"
    echo "  ght --help"
    echo "  git task man --install   # optional: makes bare 'git task --help' work too"
else
    echo "Installed, but $CARGO_BIN isn't on your PATH yet."
    echo "Add this to ~/.zshrc or ~/.bashrc, then restart your shell:"
    echo "  export PATH=\"$CARGO_BIN:\$PATH\""
fi
