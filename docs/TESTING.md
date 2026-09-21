# Testing

Manual and dev-time testing notes. For automated test commands (Vitest, `cargo test`, BDD
suites), see [AGENTS.md](../AGENTS.md#testing). For "this broke, here's the fix" entries, see
[docs/TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## Manual QA walkthrough

- **Exercise the app from a dev build** (`bun run tauri dev`): import/scan a folder,
  play/pause/seek/volume, create a playlist, edit tags, check the equalizer, and anything
  specific to whatever you changed.
- **Real-hardware smoke test** — real audio device, real playback, tags, playlists, equalizer:
  ```bash
  cargo test --test smoke_test -- --ignored --nocapture
  ```
  (run from `src-tauri/`)

## Re-testing the first-run welcome screen / walkthrough tour

Both are gated by one-off flags in the `app_state` table of your dev database (`welcome_seen`,
`walkthrough_completed`) — once set they won't show again on relaunch. Clear them with:

```bash
sqlite3 <path-to-luminous.db> "DELETE FROM app_state WHERE key IN ('welcome_seen', 'walkthrough_completed');"
```

The db lives at `%APPDATA%\org.luminous.music\luminous.db` on Windows,
`~/.local/share/org.luminous.music/luminous.db` on Linux (respects `LUMINOUS_DATA_DIR` if set —
see `src-tauri/src/paths.rs`).

## Windows UI automation

- **Windows e2e smoke test (`bun run test:e2e:windows`, `e2e/run-smoke.ts`)**: drives the real
  built app (real Rust backend, real IPC, real SQLite) through
  [`tauri-driver`](https://github.com/tauri-apps/tauri-driver) + `msedgedriver`, as opposed to
  the IPC-mocked `take-screenshots.ts`. One-time setup: `cargo install tauri-driver --locked`,
  then download the `msedgedriver` build matching your installed WebView2 Runtime version
  (`(Get-ItemProperty "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}").pv`
  in PowerShell gives the version; download
  `https://msedgedriver.microsoft.com/<version>/edgedriver_win64.zip` and drop
  `msedgedriver.exe` into `~/.cargo/bin` — already on `PATH` from the `cargo install` above, and
  machine-wide rather than tied to one worktree). The script builds the app itself via
  `tauri build --debug --no-bundle` (skips the slow installer step, just compiles the binary) —
  both flags matter. It has to go through the `tauri` CLI specifically, not a plain
  `cargo build`: only the CLI's build pipeline embeds `frontendDist` into the binary, otherwise
  it loads `devUrl` (`http://localhost:1420`) and shows a blank/network-error page since this
  test runs no dev server there to load from. And it has to be a **debug**-profile build, not
  release: `build_prevent_default_plugin()` in `lib.rs` enables
  `tauri-plugin-prevent-default`'s full flag set only in release builds
  (`cfg!(debug_assertions)` false), which disables WebView2 devtools/accelerator keys — and that
  blocks the CDP channel `msedgedriver` needs to control the page, so the window just hangs at
  `about:blank` forever (and can eventually report "tab crashed"). `--debug` keeps devtools
  enabled by carving `DEV_TOOLS` out of that flag set. Each run points the app at a fresh temp
  directory via `LUMINOUS_DATA_DIR` (see `src-tauri/src/paths.rs`) so it never reads or writes
  your real library/database — never unset that env var when experimenting with this harness
  manually.
- **`bunx tsx scripts/inspect-app.ts` (dev-time app inspection, not a CI test)**: same one-time
  setup as the e2e smoke test above. Gives an agent (or a human) a way to launch the real app
  and visually inspect it while working on an issue — `start [--real]`, `click --css "<sel>"` /
  `click --text "<text>"`, `hover --css "<sel>"` / `hover --text "<text>"` (moves the pointer via
  the WebDriver Actions API so `:hover`/group-hover CSS actually engages, unlike a plain click —
  no separate "unhover"; start a fresh session or hover a neutral element to clear it),
  `type --css "<sel>" "<text>"`, `screenshot <path>` (add `--css "<sel>"` / `--text "<text>"` to
  crop to just that element via the driver's own element-screenshot endpoint — no manual
  devicePixelRatio math, and much easier to confirm pixel-level detail like a hover outline than
  eyeballing a full-page shot), `source`, `url`, `stop`. Each subcommand is a separate process;
  session state persists to `.inspect-session.json` (gitignored) between calls. `--real` points
  it at your actual library instead of an isolated temp dir — use it only for read-only visual
  comparisons, never for clicks that mutate state.

  For known failure modes of these two tools (flaky `--real` sessions, blank/crashing windows
  under GPU/session isolation), see [docs/TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## Remote devtools for headless/agent debugging

An agent (or a developer without desktop access to the running window) can inspect the live
webview's Console/DOM/Network state without driving the app through WebDriver. This is automatically
enabled in dev builds via `package.json`'s `tauri` script (`LUMINOUS_REMOTE_DEVTOOLS=1`), and is a no-op
in release builds regardless (`remote_devtools_enabled()` in `src-tauri/src/lib.rs`).

Launch the dev server normally:

```bash
bun run tauri dev
```

(Or explicitly pass `LUMINOUS_REMOTE_DEVTOOLS=1` / `$env:LUMINOUS_REMOTE_DEVTOOLS = "1"` if running `tauri dev` directly without `bun run tauri`.)

Then, from a browser (e.g. Claude's Browser pane — `mcp__Claude_Browser__navigate`), open
`http://127.0.0.1:9222`:

- **Linux (WebKitGTK)**: this lists inspectable views; open one to get the full Web Inspector
  (Console/DOM/Network) as a normal webpage.
- **Windows (WebView2)**: `http://127.0.0.1:9222/json` lists Chrome DevTools Protocol targets;
  each has a `devtoolsFrontendUrl` that serves the Chrome DevTools UI over plain http.

Nothing is exposed unless the env var is set, and it's a no-op in release builds regardless
(`remote_devtools_enabled()` in `src-tauri/src/lib.rs`).

**Limitation**: a browser attached to the inspector frontend sees *that page's own* console via
its own tooling, not Luminous's — to read Luminous's actual console/network/DOM state, read the
Console/DOM/Network panels rendered inside the inspector UI itself (e.g. via a page-text or
screenshot read), not a structured log feed.

### CLI monitoring and diagnostics (`scripts/monitor-cdp.ts`)

For programmatic inspection without opening a browser, use the CDP monitoring script (`bun run monitor-cdp` or `bun run scripts/monitor-cdp.ts`):

```bash
# Check renderer health and event loop latency
bun run monitor-cdp

# Continuous watch mode (reports latency, document.title, and visibility state every 2s)
bun run monitor-cdp --watch

# Evaluate arbitrary JavaScript inside the running WebView2
bun run monitor-cdp --eval "document.title"
bun run monitor-cdp --eval "document.querySelectorAll('canvas').length"

# Test window minimization, background responsiveness, and restoration
bun run monitor-cdp --minimize --duration 15
```

This verifies that the WebView2 renderer thread does not lock up during background playback, window occlusion, or minimization (#1052).

