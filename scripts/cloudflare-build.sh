#!/usr/bin/env bash
# Workers Builds runs this before `wrangler deploy`. Its image has Node and Bun
# but no Rust, so this installs the toolchain and the matching worker-build.
set -euo pipefail

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"

# Reads rust-toolchain.toml.
rustup toolchain install

version="$(cargo metadata --locked --format-version 1 | node -e "
  let text = '';
  process.stdin.on('data', (chunk) => { text += chunk; });
  process.stdin.on('end', () => {
    console.log(JSON.parse(text).packages.find((crate) => crate.name === 'worker').version);
  });
")"
cargo install worker-build --locked --version "$version"
