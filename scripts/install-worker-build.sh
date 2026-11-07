#!/usr/bin/env bash
# Installs the worker-build that matches the worker crate in Cargo.lock.
set -euo pipefail

version="$(cargo metadata --locked --format-version 1 | jq -r '.packages[] | select(.name == "worker") | .version')"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT

# Fetched by hand so its lock file can be changed below, with the check cargo would do.
checksum="$(curl -fsSL https://index.crates.io/wo/rk/worker-build | jq -r --arg version "$version" 'select(.vers == $version) | .cksum')"
curl -fsSL "https://static.crates.io/crates/worker-build/worker-build-$version.crate" -o "$dir/crate.tgz"
echo "$checksum  $dir/crate.tgz" | sha256sum --check --quiet
tar -xzf "$dir/crate.tgz" -C "$dir" --strip-components=1

# worker-build 0.8.7 shipped with yoke-derive 0.8.3 in its lock file, which was yanked later.
# Its main branch already moved on, so newer releases skip this.
if [ "$version" = 0.8.7 ]; then
  cargo update --manifest-path "$dir/Cargo.toml" yoke-derive
fi

cargo install --locked --force --path "$dir"
