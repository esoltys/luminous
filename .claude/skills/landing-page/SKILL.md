---
name: landing-page
description: Update the Luminous landing page (esoltys.dev/luminous/) for a new release — screenshots from the user guide, What's New from the release notes, version, llms.txt — in both EN and FR
---

Update the landing page for release $ARGUMENTS (e.g. `2.5.0`; if omitted, use the version in
`package.json`).

The page lives in a **different repo**: `esoltys/esoltys.github.io`, under `luminous/`. Its
`.agents/AGENTS.md` ("Luminous landing page" section) is the source of truth for the page's
structure and i18n rules — re-read it before starting; if it disagrees with this file, it wins
and you should tell the user this skill needs updating. Inputs all come from this repo:
`docs/release-notes/vX.Y.Z.md` and `docs/user-guide/screenshots/`.

Run this after the release PR has merged, so the screenshots and notes on `origin/main` are the
released ones. It can run from a cloud session (unlike `release`).

## 1. Set up

- Locate a clone of `esoltys/esoltys.github.io` (locally usually a sibling of this repo; in a
  cloud session attach it with `add_repo`). `git status --short` must be clean.
- `git fetch origin main` in both repos, then branch the site repo from `origin/main` (e.g.
  `luminous-vX.Y`). All changes ship via PR — never push the site's `main` directly.
- Read `docs/release-notes/vX.Y.Z.md`, and in the site repo read `luminous/index.html`,
  `luminous/i18n/fr.js`, `luminous/llms.txt`, and the "Card sections" part of
  `luminous/styles.css`.

## 2. Screenshots

The user guide's `docs/user-guide/screenshots/<name>-EN.png` / `<name>-FR.png` map one-to-one
onto the site's `luminous/landing-assets/` (same filenames). Don't assume they were already
copied — compare by hash:

```bash
SITE=<path to esoltys.github.io>
for f in $(git ls-tree --name-only origin/main docs/user-guide/screenshots/); do
  b=$(basename "$f")
  a=$(git show "origin/main:$f" | sha1sum | cut -c1-8)
  l=$([ -f "$SITE/luminous/landing-assets/$b" ] && sha1sum < "$SITE/luminous/landing-assets/$b" | cut -c1-8 || echo NEW)
  [ "$a" = "$l" ] || echo "$b $l"
done
```

- Copy every differing file over its same-named counterpart
  (`git show origin/main:<path> > $SITE/luminous/landing-assets/<name>`). `NEW` files only
  need copying if the page will reference them.
- `theme-dynamic-*.png` (the theme slideshow) aren't in the user guide — leave them alone.
- Check the copied images' pixel sizes still match the `width`/`height` attributes on their
  `<img>` tags (EN and FR), and fix the attributes if a capture size changed:

```bash
cd $SITE && python3 - <<'E'
import re, struct
html = open('luminous/index.html').read()
for m in re.finditer(r'src="\./landing-assets/([^"]+\.png)"[^>]*?width="(\d+)" height="(\d+)"', html):
    for name in {m[1], m[1].replace('-EN.', '-FR.')}:
        w, h = struct.unpack('>II', open('luminous/landing-assets/' + name, 'rb').read()[16:24])
        if (w, h) != (int(m[2]), int(m[3])): print('MISMATCH', name, w, h, 'vs', m[2], m[3])
print('dimension check done')
E
```

- Read `home-EN.png` (the hero) and one screenshot that shows a headline feature to confirm
  they're the new captures.

## 3. Version

- `luminous/index.html`: the hero badge (`<p class="hero__badge">Version X.Y</p>`) and the
  JSON-LD `"softwareVersion"`. Use `X.Y` (drop `.0` patch) to match the existing style.
- The home page `index.html` shares the `SoftwareApplication` `@id` but has no
  `softwareVersion`; only touch it if its description/operatingSystem became untrue.

## 4. What's New (point form)

The section is a `.card-grid.card-grid--wide` holding **one card per recent version**, each a
short bulleted `<ul class="release-list">`. Keep it this way — no one-card-per-feature grids
(too long).

- Add a card for the new version **first**, with the `whatsNew.badge` "New" badge in its
  `.card__heading`. Remove the badge from the previous card.
- Keep the previous version's card while its release is still recent (most users haven't seen
  it). Keep at most two cards; if a third would appear, ask the user whether to drop the oldest.
- Bullets: user-facing highlights only, picked from the release notes' feature sections — skip
  "Under the Hood", architecture refactors, and bug fixes. Aim for ~8–10 bullets, each a short
  phrase (roughly ≤ 10 words, no trailing period), e.g. "Opus playback", "Repeat Album mode".
- Keys: title `whatsNew.vXY.title`, bullets `whatsNew.vXY.<camelCaseName>` (e.g.
  `whatsNew.v25.opus`). Every `<li>` gets its own `data-i18n`.
- Heading stays `What's New` (`whatsNew.title`) — no version number in it.

Then scan the feature rows and feature grid for copy the release made stale or incomplete (e.g.
a claim a new feature contradicts). Only change what's actually wrong or clearly worth a line
(a headline feature with a matching new screenshot can get a feature row); propose larger copy
changes to the user before writing them.

## 5. French (`luminous/i18n/fr.js`)

English lives only in the markup; `fr.js` holds French for every `data-i18n` /
`data-i18n-html` key. A missing key silently shows English, so:

- Add a real French translation for every new key (not a copy of the English). Reuse the
  existing terms: bibliothèque, liste(s) de lecture, balises, pochette/illustrations,
  « Nouveau ». Proper nouns (Flatpak, Opus, Moment Mix, MusicBrainz Picard) stay as-is.
- Delete keys that no longer exist in the markup.
- Confirm there are no missing or orphaned keys, and that the file still parses:

```bash
cd $SITE && python3 - <<'E'
import re
keys = set(re.findall(r'data-i18n(?:-html)?="([^"]+)"', open('luminous/index.html').read()))
fr = set(re.findall(r'^\s*"([^"]+)":', open('luminous/i18n/fr.js').read(), re.M))
print('missing in fr:', sorted(keys - fr)); print('orphaned in fr:', sorted(fr - keys))
E
node -e "import('./luminous/i18n/fr.js').then(m => console.log('fr.js ok,', Object.keys(m.default).length, 'keys'))"
```

## 6. `luminous/llms.txt`

- Summary line: "Currently at version X.Y."
- Add a `## What's New in X.Y` list above the previous one (a sentence per bullet is fine
  here — it's for LLMs, so more detail than the page). Drop a What's New section only when its
  card leaves the page.
- Update "Core Features" for anything that's now core (e.g. a new format or source).
- Check "Tech Stack & Architecture" still matches AGENTS.md's Tech Stack (it once said React —
  it's SvelteKit + Svelte 5).

## 7. Verify

- `bunx html-validate luminous/index.html` (from the site repo root).
- Serve the site over HTTP (`main.js` is an ES module, so `file://` won't run it):
  `python3 -m http.server 8765` from the site root, then with Playwright (Chromium) load
  `http://localhost:8765/luminous/` at 1280px and 390px wide, in EN and after clicking
  `button[data-lang=fr]`. Screenshot the hero and `section[aria-labelledby=whats-new-title]`
  and **read the images**: badge text, both cards, no overflowing French strings.
- Console: no "missing key" warnings. Font/CDN load failures are expected in a sandbox.
  `document.documentElement.scrollWidth` > viewport at 390px is a pre-existing quirk (body has
  `overflow-x: hidden`, page can't scroll) — compare against `main` before treating it as new.

## 8. Ship

- `git status --short` — only `luminous/` files should change. Commit
  `feat: update Luminous landing page for vX.Y`.
- **No AI attribution anywhere** (AGENTS.md → Branching & PR Rules): no commit trailers, no
  "Generated with/by Claude Code" or session links in the PR body. After opening the PR,
  re-read its body — tools can append a footer automatically — and edit it out.
- PR against the site's `main`, body as unwrapped paragraphs/bullets: what changed (version,
  What's New, which screenshots were refreshed, FR, llms.txt) and how it was verified. The site
  repo has no CI; GitHub Pages deploys on merge.
