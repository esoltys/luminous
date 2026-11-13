# UI/UX Design Conventions

Moved out of `AGENTS.md`; read this before any UI, animation, copy or locale change.

- **Responsive breakpoints: viewport for the shell, container for what lives inside it.** Window-size
  tiers are declared once as `--breakpoint-*` in `src/app.css` and mirrored by `BREAKPOINT_*_PX` in
  `src/lib/constants.ts` (`breakpoints.test.ts` fails if they drift): `xs` 420, `sm` 640 (compact to medium),
  `md` 768 (right panel), `lg` 1024 (sidebar). Height tiers are `HEIGHT_BREAKPOINT_*_PX` (JS-only, 160/600).
  Use them for app-shell decisions only. Anything that depends on the space a component actually gets (it
  moves with the sidebar and right panel) uses a container query with Tailwind's named sizes (`@sm`, `@3xl`),
  never a viewport prefix or an arbitrary `min-[Npx]` / `@min-[Npx]` value. Structural show/hide goes through
  `windowLayoutStore`; pure styling goes through CSS. Never both for one decision.

- **Toast persistence**: Toasts must never auto-dismiss unless an explicit `durationMs` is passed by the
  caller. By default, toasts stay visible until the user clicks the `X` button.

- **Explicit-action-only celebrations**: Micro-animations (heart pulse, confetti, etc.) must fire only in
  response to a deliberate user interaction (e.g., a click handler). Never trigger them from a reactive
  `$effect` or a prop-change watcher — doing so causes false positives when the track changes and a
  different (already-favourited) song loads. Enforced by `celebration-in-effect`.

- **Motion budget**: every animation must name its purpose — feedback, spatial continuity, state
  legibility, bridging a jarring change, first-run explanation, or rare delight. If it has none,
  don't build it.
  - **Frequency decides whether it animates.** 100+/day (seek, volume drag, scroll, column resize,
    row selection): none. Tens/day (panel toggle, view switch): near-imperceptible. Occasional
    (dialogs, theme switch): standard. Rare/first-run (welcome, celebrations): the only place for delight.
  - **Keyboard shortcuts snap.** The key asks for the result, not the journey — wrap layout changes
    triggered from a hotkey in `instantly()` in `+layout.svelte`.
  - **Durations**: press/toggle 100–160 ms, tooltip/popover 125–200 ms, menu/list 150–250 ms,
    panel/drawer/dialog 200–300 ms. Nothing ordinary exceeds 300 ms; only rare-tier
    `animations.css` celebrations may.
  - **Easing**: ease-out for entrances *and* exits (`cubicOut` on `out:` too — Svelte eases the
    outro's progress, so `cubicIn` there is an ease-in exit), ease-in-out for movement, `ease` for
    hover/colour. Never ease-in.
  - **Retriggerable effects use CSS transitions, not keyframes**, so a re-trigger continues from
    where the element is instead of restarting.
  - **Visualisers are signal.** Decorative chrome must never pulse, breathe or drift in a way that
    could be mistaken for the spectrum or waveform.

- **Reduced motion**: import `fly`/`slide`/`scale`/`fade` from `src/lib/utils/motion.ts`, never from
  `svelte/transition` — the wrappers drop transforms (and make `slide` instant) under
  `prefers-reduced-motion`, which CSS can't do for JS transitions. CSS transitions are covered by the
  `!important` reduce block at the end of `animations.css`. Reduced means fewer and gentler, never zero: keep opacity/colour.

- **No hover-only information**: a native `title=` is fine when it repeats something already visible
  (a button's own text/`aria-label`, truncated text whose full value is shown elsewhere). When the hint
  is the only place that information appears, use `HelpTip.svelte` (focusable, opens on hover *and*
  keyboard focus, closes on Escape/press/wheel, read as a description) or show the text inline — a
  `title` on a non-focusable element never appears for keyboard users.

- **State is never colour-only**: warnings, errors, status and destructive actions use
  `text-brand-text-primary`/`-secondary` (never `text-red-*`, `bg-amber-*`, `border-emerald-*` and the
  like — a theme can share that colour, and palette values ignore the theme's contrast) and say what
  they mean with an icon, wording, a sign or a shape change (filled vs hollow). Enforced by
  `bun run lint:state-colours`, with no allowlist (#1878).

- **Context-aware completion messages**: End-of-queue or completion toasts must include the name of what
  finished (e.g., "Jazz Classics complete"), not generic text. The `playerStore.activeContextName` field
  carries this name; auto-playlists (Favourites, Recently Added, etc.) must pass their `displayName` to
  `playerStore.playSongs()` so the context propagates correctly.

- **The UI never re-derives a law the backend owns**: show the backend's own value or call its
  accessor — never reimplement its formula in a component. A copy drifts, and is usually right only
  at the endpoints nobody spot-checks (e.g. the EQ curve drew shelf bands as bells, #1248). Slider
  bounds are part of the law: read them from the backend rather than retyping them.

- **A cached verdict is shown only next to what it answered**: if a control's label or enabled state
  depends on an expensive check, key the result on its inputs, re-check the key on each read, and
  show "stale" or disable the control when it no longer matches.

- **`<select>`: assignment or reload?** Re-picking the selected option fires no `change`. If picking
  re-applies something, give a second path — a reload button, or a "Custom" state that makes
  re-picking a real change (as the EQ preset picker does).

- **No hand-typed versions, counts or dates in the UI**: derive them from one constant or the build.

- **Sentence-case labels, no tracked uppercase**: labels, headings and column headers are sentence case in
  every locale, including the English source string ("Sample rate", not "Sample Rate"). Hierarchy comes from
  size, weight and colour via `label-field` (form labels), `label-heading` (section and column headers) and
  `label-small` (chips, captions) in `src/app.css`, which set size and weight; add the colour at the call
  site. Never use `uppercase` or `tracking-wide*`. Codes and abbreviations that are data rather than labels
  (hex values, language codes, file types) use `code-caps`. Enforced by `no-uppercase-label`.

- **A disabled or inert control is a defect**: don't ship a control whose value nothing consumes.

- **Icon semantics**: Avoid icons that imply system-level tracking or achievement recording (e.g.,
  `<Trophy>`). For milestone/completion moments, prefer neutral icons like `<Star>` that convey
  "special" without implying a leaderboard or achievement system.

- **Instructive, not descriptive, copy voice**: Hint/help text under a toggle, button, or tab should
  tell the user what to do or what happens when they act ("Scan watched folders for new files"), not
  describe the feature in third person ("Scans watched folders for new files") or lead with an adverb
  ("Automatically scans..."). Match the imperative voice of the control's own label. This applies to
  `src/lib/locales/en.ts` hint/tooltip strings specifically — section headings and status/value labels
  are fine as descriptive text.

- **i18n is not optional**: every new user-facing string key added to `src/lib/locales/en.ts` must be
  added to every other base locale file in `src/lib/locales/` (`fr`, `de`, `es`, `it`, `ru`, `uk`;
  list the directory, since the set grows), including the full-file `fr-FR`, with an actual translation,
  not a copy of the English text, in the same change. The partial `en-US` / `en-GB` variants fall back
  to their base and only carry keys that differ. `i18n.t()` falls back to English when a key is missing,
  but this is strictly a runtime fallback for safety — never a substitute for translating. Completeness
  (no missing keys, no stale keys), placeholder token matching, and non-identical translations (unless
  explicitly allowlisted in `IDENTICAL_OK`) are enforced in CI by `src/lib/locales/locales.test.ts`.

- **Translation guidelines**: adding a locale, per-language terminology authorities, register and
  placeholder/`{count}` conventions live in [docs/TRANSLATIONS.md](TRANSLATIONS.md).

- **French is Canadian French**: for `src/lib/locales/fr.ts` and the French user guide, check terms in
  the OQLF's Grand dictionnaire terminologique (vitrinelinguistique.oqlf.gouv.qc.ca) first, then
  TERMIUM Plus. Established terms: *étiquette* (tag), *diffusion en continu* (streaming), *bogue*
  (bug), *liste de lecture* (playlist), *simple* (single), *zone de notification* (system tray),
  *palmarès* (chart), *image dans l'image* (picture-in-picture). Typography: a space before `:` and
  inside « », none before `?`, `!` or `;`.
