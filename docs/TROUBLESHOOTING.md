# Troubleshooting

For manual/dev-time testing guidance (re-testing onboarding flows, clean install testing, CDP monitoring), see [docs/TESTING.md](TESTING.md).

- **Tauri dev won't start**: ensure the git hook is installed (`bun run install:git-hooks`), check the Rust toolchain (`cargo --version`), or clear the Tauri cache (`rm -rf src-tauri/target`).
- **`Failed to unregister class Chrome_WidgetWin_0. Error = 1411` in dev server console**: this is a benign diagnostic log from Chromium's window cleanup code (`ui\gfx\win\window_impl.cc`) via the embedded Microsoft Edge WebView2 runtime. It appears during window teardown, app shutdown, or dev server restarts when Chromium calls `UnregisterClass` on a window class already unregistered or released by the OS (Win32 error `1411` = `ERROR_CLASS_DOES_NOT_EXIST`). It is harmless, causes no memory leaks or instability, and can be safely ignored.
- **Frontend type errors after a dependency update**: run `bun run check`. Svelte 5 Runes don't need destructuring (`$`-prefixed variables are already reactive).
- **An element is transparent (or loses its border/shadow) even though a rule clearly gives it a colour — often in only one theme**: suspect a broken `var()`, not a losing cascade. A `var(--x)` naming a property that isn't defined still *wins* the cascade, then is invalid at computed-value time and silently computes to `unset` — no console error. In DevTools the declaration shows as applied but the computed value is the initial one. `bun run lint:css-vars` (part of `bun run check`) catches reads of properties defined nowhere; `bun run lint:css-vars --unused` also lists fallback-guarded reads whose name is defined nowhere. Properties set only at runtime (e.g. `--glass-*` from `theme.svelte.ts`) can also be missing *until* the code that sets them runs.
- **Audio playback crackling/stuttering**: verify no allocations happen in the audio output callback (`OutputRenderer` in `crates/luminous-audio`); profile with `cargo flamegraph` if CPU-bound.
- **Tests fail in CI but pass locally**: Vitest in CI uses jsdom (not a browser) — confirm jsdom-compatible selectors; for Rust, check for platform-specific code (especially file paths).
- **`bun run tauri:windows:build` fails with `Executable not found: ...\src-tauri\target\x86_64-pc-windows-msvc\release\LuminousMusicPlayer.exe`**: this repo is a Cargo workspace (`src-tauri` is a member), so `cargo build`/`tauri build` write artifacts to the workspace-root `target/`, not `src-tauri/target/`. `tauri-windows-bundle` (which builds the MSIX) only checks `src-tauri/target/` unless `CARGO_TARGET_DIR` is set — the release workflow (`.github/workflows/release.yml`) sets it explicitly. Locally, run `CARGO_TARGET_DIR="$PWD/target" bun run tauri:windows:build` (or `$env:CARGO_TARGET_DIR = "$PWD\target"` in PowerShell) from the repo root. If your local Cargo config sets a shared `build.target-dir`, point `CARGO_TARGET_DIR` at that dir instead (`cargo metadata --format-version 1 --no-deps` prints it as `target_directory`).
- **Sideloading a locally-built `.msix` for manual testing**: `bun run tauri:windows:build` produces an unsigned `.msix`, and Windows refuses to install *any* unsigned MSIX, even with Developer Mode on — there's no `Add-AppxPackage -AllowUnsigned` equivalent. You need a self-signed cert matching the `publisher` CN in `src-tauri/gen/windows/bundle.config.json`, trusted locally, used to sign the package with `signtool` (from the Windows SDK, usually already installed at `C:\Program Files (x86)\Windows Kits\10\bin\<version>\<arch>\signtool.exe`):
  ```powershell
  $cert = New-SelfSignedCertificate -Type Custom -Subject "CN=<publisher-CN-from-bundle.config.json>" `
    -KeyUsage DigitalSignature -FriendlyName "Luminous Dev Signing" `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3","2.5.29.19={text}")
  Export-PfxCertificate -Cert $cert -FilePath luminous-dev-signing.pfx -Password (ConvertTo-SecureString -String "devtest" -Force -AsPlainText)
  Export-Certificate -Cert $cert -FilePath luminous-dev-signing.cer
  # A self-signed leaf alone isn't enough — Windows validates the chain up to a trusted root,
  # and needs LocalMachine (not just CurrentUser), so this needs an elevated PowerShell:
  Import-Certificate -FilePath luminous-dev-signing.cer -CertStoreLocation Cert:\LocalMachine\Root
  Import-Certificate -FilePath luminous-dev-signing.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
  & "C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\signtool.exe" sign /fd SHA256 /f luminous-dev-signing.pfx /p devtest "target\msix\<file>.msix"
  Add-AppxPackage -Path "target\msix\<file>.msix"
  ```
  Never commit the generated `.pfx`/`.cer` (gitignored) — they're a throwaway local dev cert, not the real release signing key.
- **Only commit `docs/user-guide/assets/{locale}/screenshots/{light,dark,dynamic}/*.png` when the task is refreshing the docs screenshots.** Capture Studio in `luminous-store` (`bun run studio`) exists to produce those assets; it is not a way to check everyday UI work. To verify a UI change, run `bun run tauri dev` and attach to the remote devtools port (see [TESTING.md](TESTING.md)). If you regenerate screenshots while testing, revert the resulting `docs/user-guide/assets/**/*.png` changes before committing unless that was the explicit goal.
