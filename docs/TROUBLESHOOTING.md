# Troubleshooting

- **Tauri dev won't start**: ensure the git hook is installed (`bun run install:git-hooks`), check the Rust toolchain (`cargo --version`), or clear the Tauri cache (`rm -rf src-tauri/target`).
- **Frontend type errors after a dependency update**: run `bun run check`. Svelte 5 Runes don't need destructuring (`$`-prefixed variables are already reactive).
- **Audio playback crackling/stuttering**: verify no allocations happen in the `audio.rs` playback loop; profile with `cargo flamegraph` if CPU-bound.
- **Tests fail in CI but pass locally**: Vitest in CI uses jsdom (not a browser) — confirm jsdom-compatible selectors; for Rust, check for platform-specific code (especially file paths).
- **`bun run take-screenshots` (`scripts/take-screenshots.ts`)**: regenerates `docs/user-guide/screenshots/*.png` by driving the app in Playwright against the mocked Tauri IPC bridge (`scripts/tauri-ipc-mock.ts`). In a remote/sandboxed environment where Playwright's own downloaded browser is unavailable and a Chromium is pre-installed at a fixed path instead, `chromium.launch()` needs `executablePath` pointed at it to run at all — but that path is environment-specific, so never commit it; apply it locally, capture, then revert before committing. If the mock is missing a command the app now calls, `invoke()` warns `[Tauri Mock] Unhandled command: ...` in the page console and returns `null` rather than failing capture outright — but a command whose *return value* the store actually reads (e.g. `get_library_snapshot`) will throw and abort store init instead, so treat any "Unhandled command" warning during a screenshot run as something to add a handler for in `tauri-ipc-mock.ts`, not just noise.
- **`bun run tauri:windows:build` fails with `Executable not found: ...\src-tauri\target\x86_64-pc-windows-msvc\release\LuminousMusicPlayer.exe`**: this repo is a Cargo workspace (`src-tauri` is a member), so `cargo build`/`tauri build` write artifacts to the workspace-root `target/`, not `src-tauri/target/`. `tauri-windows-bundle` (which builds the MSIX) only checks `src-tauri/target/` unless `CARGO_TARGET_DIR` is set — the release workflow (`.github/workflows/release.yml`) sets it explicitly. Locally, run `CARGO_TARGET_DIR="$PWD/target" bun run tauri:windows:build` (or `$env:CARGO_TARGET_DIR = "$PWD\target"` in PowerShell) from the repo root.
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
- **Do not commit `docs/user-guide/screenshots/*.png` output from `take-screenshots.ts`.** The committed screenshots are captured manually against a real library, not the mocked fixture data — running the script is for verifying the capture flow itself still works (dev server boots, every page renders, every mock command resolves), not for producing docs assets. If you run it while debugging the script or the mock, revert the resulting `docs/user-guide/screenshots/*.png` changes before committing.
