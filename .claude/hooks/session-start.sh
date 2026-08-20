#!/bin/bash
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# System libraries needed to `cargo check`/`cargo build`/`bun run tauri dev`
# on Linux. The first four are documented in AGENTS.md's "System
# Dependencies (Linux)" section; the rest (GTK/WebKit dev headers) are
# additionally required to compile the `gdk-sys`/`webkit2gtk` crates but
# aren't preinstalled in this environment's base image.
apt-get update -qq
apt-get install -y -qq \
  libasound2-dev \
  libssl-dev \
  pkg-config \
  libayatana-appindicator3-dev \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev

# Xvfb + screenshot tooling so a GUI verification pass (launching the app
# and capturing its window at various sizes) is possible without extra
# per-session setup — this app is a Tauri/GTK desktop window, not a web
# server, so there's no browser-based alternative to driving it visually.
apt-get install -y -qq xvfb xdotool imagemagick x11-utils

# JS/TS dependencies (bun, not npm/yarn/pnpm — see AGENTS.md "Package Manager").
bun install

# Rust dependencies — warms cargo's registry/target cache so the first
# `cargo check`/`tauri dev` in the session doesn't compile ~750 crates cold.
(cd src-tauri && cargo check)
