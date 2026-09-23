# Flatpak

Luminous publishes a Flatpak build (`org.luminous.music`) alongside the Windows MSI/MSIX on every
release, via `.github/workflows/release.yml`'s `flatpak-build` job. It's distributed two ways: a
standalone `.flatpak` bundle attached to the GitHub release, and an entry in a shared, multi-project
OSTree repo hosted as static files at `https://esoltys.dev/flatpak/` (repo:
[`esoltys/esoltys.github.io`](https://github.com/esoltys/esoltys.github.io)) — the same repo/signing
setup other future projects can reuse instead of standing up their own.

## Install via the shared repo (recommended — gets auto-updates)

```bash
flatpak remote-add --if-not-exists esoltys https://esoltys.dev/flatpak/esoltys.flatpakrepo
flatpak install esoltys org.luminous.music
```

Future updates: `flatpak update org.luminous.music`.

## Install the standalone bundle

Download `LuminousMusicPlayer.flatpak` from the [latest release](https://github.com/esoltys/luminous/releases/latest), then:

```bash
flatpak install --bundle LuminousMusicPlayer.flatpak
```

This does not track the shared repo for updates — re-download on each release, or switch to the
remote-based install above.

## Build locally

Requires `flatpak` and `flatpak-builder`, plus the GNOME runtime/SDK and the Rust/Node SDK extensions:

```bash
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable org.freedesktop.Sdk.Extension.node20
```

```bash
flatpak-builder --repo=/tmp/luminous-repo --force-clean /tmp/luminous-build flatpak/org.luminous.music.yml
flatpak build-bundle /tmp/luminous-repo LuminousMusicPlayer.flatpak org.luminous.music
flatpak install --bundle LuminousMusicPlayer.flatpak
```

### Why the build module has network access

Flathub is the party that enforces network-isolated builds (for reproducibility/supply-chain
review during submission). This repo is self-hosted, not submitted to Flathub, so
`flatpak/org.luminous.music.yml`'s `luminous` module is granted `--share=network` directly and
just runs `bun install`/`bun run tauri build` normally — no `cargo-sources.json`/`node-sources.json`
generation step. This was a deliberate simplification: `flatpak-node-generator` (the standard tool
for turning a JS lockfile into offline Flatpak sources) only supports npm/yarn/pnpm lockfiles, not
Bun's `bun.lock`, so the offline route would have needed vendoring `node_modules/` as a pre-fetched
archive source instead — more moving parts for no benefit outside a Flathub submission. If Luminous
ever does go through Flathub, this module will need to be reworked back to offline sources (Cargo's
via `flatpak-cargo-generator`, which works fine against `Cargo.lock`; JS via a vendored
`node_modules/` archive, since the generator still won't support Bun).

## Sandbox notes

- Audio plays through PipeWire/PulseAudio via `--socket=pulseaudio` (CPAL's usual backend under
  Flatpak).
- Adding a music library folder goes through the XDG Desktop Portal file picker (Tauri's dialog
  plugin uses this automatically under Flatpak), which grants persistent access to just the
  folders you pick — Luminous does not request broad `--filesystem=home` access.
- Lyrics (LRCLIB, Lyrics.ovh), cover art (iTunes), and MusicBrainz lookups need network access,
  granted via `--share=network`.

## Publishing (maintainers)

The shared repo needs a one-time bootstrap in `esoltys/esoltys.github.io` before the
`flatpak-build` job's publish step can run (it skips cleanly if `FLATPAK_REPO_GPG_KEY` isn't set):

1. `ostree init --mode=archive-z2 --repo=flatpak/repo` in that repo, and commit an
   `flatpak/esoltys.flatpakrepo` file (see that repo for the template).
2. Generate a GPG key dedicated to this shared repo (not tied to any one project):
   ```bash
   gpg --batch --full-generate-key <<'EOF'
   %no-protection
   Key-Type: RSA
   Key-Length: 3072
   Name-Real: esoltys.dev Flatpak Repo
   Name-Email: flatpak@esoltys.dev
   Expire-Date: 0
   EOF
   gpg --list-secret-keys --keyid-format long flatpak@esoltys.dev  # note the key id
   ```
3. Store the key and its id as secrets on **this** repo (`esoltys/luminous`), and paste the public
   key into `esoltys.flatpakrepo`'s `GPGKey=` field in the `esoltys.github.io` repo:
   ```bash
   gpg --export-secret-keys <key-id> | base64 -w0 | gh secret set FLATPAK_REPO_GPG_KEY --repo esoltys/luminous
   gh secret set FLATPAK_REPO_GPG_KEY_ID --repo esoltys/luminous --body "<key-id>"
   gpg --export <key-id> | base64 -w0   # -> esoltys.flatpakrepo's GPGKey= field
   ```
4. Create a fine-grained GitHub PAT scoped only to `esoltys/esoltys.github.io` (Contents:
   read/write), and store it as `ESOLTYS_IO_DEPLOY_TOKEN` on `esoltys/luminous`:
   ```bash
   gh secret set ESOLTYS_IO_DEPLOY_TOKEN --repo esoltys/luminous
   ```

`esoltys.dev` is already served by GitHub Pages from that repo's `CNAME` — no DNS change is needed.
Once the secrets exist, every tagged release's `flatpak-build` job regenerates and pushes only the
`flatpak/repo` subtree of `esoltys.github.io`, leaving other content (and any other project's own
ref in the same OSTree repo) untouched.
