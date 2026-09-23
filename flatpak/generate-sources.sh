#!/usr/bin/env bash
# Regenerates flatpak/cargo-sources.json and flatpak/node-sources.json from
# Cargo.lock and bun.lock, so `flatpak-builder` can build org.luminous.music.yml
# with no network access. Run this whenever either lockfile changes, then commit
# the two generated JSON files alongside the lockfile change.
#
# Requires: python3, pip. Uses flatpak-builder-tools (upstream, not vendored).
set -euo pipefail
cd "$(dirname "$0")/.."

TOOLS_DIR="$(mktemp -d)"
trap 'rm -rf "$TOOLS_DIR"' EXIT

git clone --depth 1 https://github.com/flatpak/flatpak-builder-tools "$TOOLS_DIR"

python3 "$TOOLS_DIR/cargo/flatpak-cargo-generator.py" \
  Cargo.lock -o flatpak/cargo-sources.json

# flatpak-node-generator's bun.lock support tracks upstream flatpak-builder-tools;
# if this fails, see the "JS offline sources" note in docs/FLATPAK.md for the
# node_modules-tarball fallback.
python3 "$TOOLS_DIR/node/flatpak-node-generator.py" \
  bun bun.lock -o flatpak/node-sources.json

echo "Wrote flatpak/cargo-sources.json and flatpak/node-sources.json"
