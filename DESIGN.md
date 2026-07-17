---
version: alpha
name: Luminous
description: >
  Dark-first desktop music player UI. Tokens below describe the default
  "Luminous" theme (see src/lib/stores/theme.svelte.ts:79 LUMINOUS_DARK_COLORS)
  — the canonical identity the app ships with, one of several runtime-
  swappable themes (see Overview).
colors:
  bg-main: "#08090c"
  bg-sidebar: "#1c1f29"
  bg-playerbar: "#191b23"
  accent: "#6f7ea9"
  accent-hover: "#8c98ba"
  accent-text: "#6f7ea9"
  accent-contrast: "#000000"
  text-primary: "#f1f3f8"
  text-secondary: "#a6adc4"
  border: "#2c2f3c"
typography:
  display:
    fontFamily: Inter
    fontSize: 36px
    fontWeight: 700
    lineHeight: 1.15
  headline-lg:
    fontFamily: Inter
    fontSize: 24px
    fontWeight: 700
    lineHeight: 1.2
  headline-md:
    fontFamily: Inter
    fontSize: 20px
    fontWeight: 600
    lineHeight: 1.25
  title:
    fontFamily: Inter
    fontSize: 18px
    fontWeight: 600
    lineHeight: 1.3
  body-md:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.5
  body-sm:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: 600
    lineHeight: 1.3
  micro-badge:
    fontFamily: Inter
    fontSize: 9px
    fontWeight: 700
    lineHeight: 1
    letterSpacing: 0.05em
rounded:
  sm: 6px
  md: 8px
  lg: 12px
  xl: 16px
  full: 9999px
spacing:
  xs: 4px
  sm: 8px
  md: 12px
  lg: 16px
  xl: 24px
  2xl: 32px
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-contrast}"
    typography: "{typography.body-sm}"
    rounded: "{rounded.md}"
    padding: 12px
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
  button-secondary:
    backgroundColor: transparent
    textColor: "{colors.text-primary}"
    typography: "{typography.body-sm}"
    rounded: "{rounded.full}"
    padding: 12px
  chip:
    backgroundColor: transparent
    textColor: "{colors.text-secondary}"
    typography: "{typography.body-sm}"
    rounded: "{rounded.full}"
    padding: 8px
  chip-active:
    backgroundColor: "{colors.border}"
    textColor: "{colors.text-primary}"
  badge:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-text}"
    typography: "{typography.micro-badge}"
    rounded: "{rounded.sm}"
  nav-item:
    backgroundColor: transparent
    textColor: "{colors.text-secondary}"
    typography: "{typography.body-md}"
    rounded: "{rounded.md}"
  nav-item-active:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-contrast}"
  card:
    backgroundColor: "{colors.bg-sidebar}"
    rounded: "{rounded.md}"
    padding: 16px
---

# Luminous

## Overview

Luminous is a dense, dark-first desktop music player (Tauri + Svelte 5). The
default aesthetic — codenamed "Luminous Dark" — favors a near-black canvas,
muted slate-blue accents, and generous use of soft rounded geometry over hard
edges, evoking a calm, focused listening environment rather than a flashy
media app.

The UI is built almost entirely from panels: a collapsible **Sidebar**, a
**TopNavigation** header, the main content canvas, an optional **RightPanel**,
and a persistent **PlayerBar** footer. All chrome and content surfaces read
colors exclusively from CSS custom properties (`--bg-main`,
`--color-accent`, etc., wired through `@theme` in `src/app.css`) rather than
literal hex values in components — this is what allows the runtime theme
switcher to work.

Theming is **not static**: users can switch between several built-in themes
(Luminous, Ruby Red, Nordic Blue, Retro Amber, System light/dark) or build a
custom one in the in-app theme editor (`DesignTools.svelte`,
`FoldersView.svelte`). Every theme is generated or validated against a
**WCAG AA 4.5:1 contrast guarantee** between accent and background — see
`clampForContrast()` / `generatePaletteFromSeed()` in
`src/lib/utils/colorUtils.ts`. This document describes the shipped default
theme; treat its *structure and rules* (spacing, radii, elevation approach,
contrast discipline) as normative for new UI, and its *specific hex values*
as the default instance of a swappable palette.

## Colors

- **bg-main (#08090c):** Near-black base canvas behind all content — the
  emptiest, quietest layer.
- **bg-sidebar (#1c1f29) / bg-playerbar (#191b23):** Slightly lighter
  "chrome" tones that separate navigation and transport controls from the
  content canvas without a hard border.
- **accent (#6f7ea9) / accent-hover (#8c98ba):** A muted, desaturated slate
  blue — the only hue used for interactive emphasis (active nav item, play
  button, primary actions, focus/selection states). Never used decoratively.
- **accent-text (#6f7ea9):** The accent color clamped to WCAG AA 4.5:1
  against `bg-main`. This is the *only* accent-derived value allowed on text
  or icons sitting directly on the main canvas — using raw `accent` for text
  risks failing contrast on generated/custom themes.
- **accent-contrast (#000000):** Heuristically derived per-theme text/icon
  color for content placed *on top of* an accent-colored surface (e.g. the
  glyph inside a filled play button).
- **text-primary (#f1f3f8) / text-secondary (#a6adc4):** Primary reads as
  near-white for titles and active content; secondary is a desaturated
  blue-gray for metadata, captions, and inactive labels.
- **border (#2c2f3c):** The sole structural line color, almost always used
  at reduced opacity (`/40`, `/50`, `/60`) rather than full strength.

## Typography

Single typeface throughout: **Inter** (`src/app.css:51`), falling back to
system UI sans. There is no secondary/display face — hierarchy comes from
size and weight, not font-family switching.

- **Display (36px / 700):** Reserved for album/artist hero titles at the top
  of detail views.
- **Headline (24px–20px / 700–600):** Section headers and dialog titles.
- **Title (18px / 600):** Card and list-group headers.
- **Body (14px / 12px, 400):** The workhorse sizes — the large majority of UI
  text in Luminous is `body-sm` (12px) for metadata-dense lists (folders,
  tags, playlists) and `body-md` (14px) for primary row labels and buttons.
- **Label (12px / 600):** Interactive text — buttons, active tab labels,
  song titles that double as links.
- **Micro badge (9px–11px / 700, tracked uppercase):** Format/quality tags
  (e.g. FLAC, 24-bit) rendered as tiny pills — the smallest text in the app,
  always bold and always uppercase to stay legible at that size.

Weight is used deliberately: `medium` (500) for inactive nav items, `semibold`
(600) for buttons and anything clickable, `bold` (700) for badges and large
numerals, `extrabold` (800) reserved for the single active lyric line in
`LyricsView` — the one place the app uses maximum emphasis.

## Layout

Luminous uses Tailwind's default 4px spacing unit, not a custom grid. Spacing
is chosen contextually rather than from a rigid column system:

- **Tight (4–8px):** Icon gaps, chip padding (`px-3 py-1`).
- **Standard (12–16px):** Button padding (`px-4 py-2`), card gaps.
- **Generous (24–32px):** Section padding inside large panels (`p-6`, `p-8`
  in `Equalizer.svelte`), separating major content blocks.

Structurally, the app is a fixed shell, not a scrolling page: `Sidebar` (fixed
width, collapsible to icon-only), `TopNavigation` (fixed 80px header), the
scrollable content canvas, an optional `RightPanel`, and a fixed 80px
`PlayerBar` footer that is always present regardless of route. Only the
content canvas scrolls; chrome stays pinned.

## Elevation & Depth

Luminous is largely **flat**: hierarchy comes from background tone steps
(`bg-main` → `bg-sidebar`/`bg-playerbar`) and low-opacity borders, not drop
shadows. Shadows are used sparingly and with intent:

- `shadow-sm`/`shadow-xs` mark subtle resting state on interactive chrome
  (active toggle buttons, outline buttons).
- `shadow-md`/`shadow-lg` mark active/hover elevation — critically, these are
  almost always **tinted with the accent color** (`shadow-brand-accent/20`,
  `shadow-brand-accent/10`) rather than plain black, giving active elements a
  soft accent glow instead of a generic drop shadow.
- `shadow-xl`/`shadow-2xl` are reserved for modal-like overlays (`TagEditor`)
  and hero album art.

One exception: when the user selects the **System** auto-theme, chrome
panels (sidebar, player bar, right panel, top nav) switch to a genuine
glassmorphism treatment via the `.glass-surface` class
(`backdrop-filter: blur(20px) saturate(180%)` plus a translucent tinted
background and specular top-edge highlight — see `src/app.css:91-121`). This
is theme-conditional, not the default look.

## Shapes

Rounded corners are used pervasively and scale with a component's visual
weight — there are no sharp (unrounded) rectangular surfaces in the app:

- **sm (6px):** Compact icon-toggle buttons (view/panel toggles in
  `TopNavigation`).
- **md (8px):** The default — most buttons, list rows, cover thumbnails,
  form inputs.
- **lg (12px):** Elevated cards and panels (theme picker cards, folder rows,
  settings tab strip).
- **xl (16px):** Large containers — hero album art, modal dialogs
  (`TagEditor`), the equalizer panel.
- **full:** Exclusively for circular controls (play/pause, avatars, the
  collapsed sidebar's icon buttons) and pill-shaped filter chips/tabs — never
  for standard content cards.

The `PlayerBar` footer is a deliberate outlier: it uses a custom `2rem`
(32px) radius to read as a distinct floating pill rather than blending into
the panel-edge language used everywhere else, reinforcing that it's a
persistent, separate transport surface.

## Components

- **Buttons:** Primary actions use `bg-accent` with `accent-contrast` text,
  `semibold`, and an accent-tinted shadow glow. Secondary/outline actions use
  a transparent background with a `border` and hover to `bg-sidebar`. Play
  controls are always circular (`rounded-full`); most other buttons are
  `rounded-lg`.
- **Chips/filter tabs** (genre/media-type filters, discography filters):
  `rounded-full`, `px-3 py-1`, `body-sm` weight `medium`. Active state swaps
  to `bg-border` + `text-primary` + `semibold` + `shadow-sm`; inactive state
  is a transparent/borderless label at reduced opacity.
- **Nav items** (Sidebar): `rounded-lg` when expanded, `rounded-xl` when
  collapsed to icon-only squares. Active item gets `bg-accent` +
  `accent-contrast` text + an accent-glow shadow; inactive items are
  `text-secondary` with a faint accent-tinted hover.
- **Cards** (album/artist covers, carousel items): `rounded-lg`, a
  half-opacity `border`, and hover states that brighten the border to full
  accent color plus an accent-tinted glow shadow — never a background-color
  change on hover.
- **Badges** (format/quality tags — FLAC, bitrate, etc.): micro-badge
  typography, `rounded` (not full), low-opacity accent background
  (`bg-accent/10`–`/15`) with a matching low-opacity accent border.
- **Modals** (`TagEditor`): `bg-sidebar`, full-opacity `border`,
  `rounded-2xl`, `shadow-2xl` — the app's only true overlay surface with
  heavy elevation.

## Do's and Don'ts

- Do read all colors from the `--color-brand-*` / `--bg-*` CSS custom
  properties (via Tailwind's `brand-*` classes) — never hardcode a theme hex
  in a component, or it breaks when the user switches themes.
- Do use `accent-text` (the contrast-clamped token), not raw `accent`, when
  coloring text or icons that sit directly on `bg-main`/`bg-sidebar`; this is
  the guarantee that keeps every theme, including user-generated ones, at
  WCAG AA.
- Do tint elevation shadows with the accent color on interactive/active
  elements instead of using a plain black shadow.
- Do reserve `rounded-full` for circular controls and pill chips — don't use
  it on standard rectangular cards or panels.
- Don't introduce a second typeface; hierarchy comes from size/weight, not
  font family.
- Don't add heavy drop shadows to flat chrome panels (sidebar, top nav,
  player bar) outside of the System theme's glass treatment — the app's
  default depth model is tonal steps and borders, not shadow stacking.
- Don't mix more than two font weights in a single view beyond
  `medium`/`semibold`; `bold`/`extrabold` are reserved for badges and the
  single active-lyric highlight, respectively.
