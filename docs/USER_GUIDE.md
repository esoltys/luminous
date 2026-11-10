# User Guide

Moved out of `AGENTS.md`; read this before editing the in-app manual.

- The in-app Help view loads the user manual from `docs/user-guide/`: one `luminous-user-guide-{EN,FR,DE,ES,IT,RU,UK}.html` per UI language plus shared `guide.css` / `guide.js`. It is plain HTML/CSS/JS maintained in this repo — edit it directly.
- Update every guide together in the same change, like locale strings.
- It must work offline: no CDN scripts, web fonts or other remote requests. Colours come from the app's theme variables (`guide.js` mirrors them from the parent window), so don't hard-code a palette.
- `bun run sync-docs` copies it into `static/` (generated, gitignored), along with the app's fonts from `src/lib/fonts/` into `static/fonts/` — the guide keeps no font files of its own. Preview from `static/`; opened straight from `docs/user-guide/` it falls back to the system font.
- Screenshots: `docs/user-guide/assets/**.png` are the captures, kept as PNG for the Microsoft Store listing and the landing page. The guides reference a lossy WebP copy (`<name>.webp`, committed next to each PNG) and only those referenced copies are synced into the app, since everything synced is embedded in the exe. The capture scripts refresh the copies automatically; after replacing a PNG by hand run `bun run guide-webp` (needs ImageMagick). `scripts/guide-assets.test.ts` fails if the embedded set exceeds its size budget (#1647).
- Guide image contract: `docs/user-guide/assets/manifest.json` defines every screenshot and expected pixel dimensions per language. `bun run sync-docs` and `src/lib/locales/guide-manifest.test.ts` validate complete parity between the guide HTML, manifest, and disk files (WebP and PNG masters) before build, failing on missing files or dimension mismatches. Run `bun scripts/guide-manifest.ts` to validate manually, or `bun scripts/guide-manifest.ts --generate` to update the manifest.
