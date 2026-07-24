---
version: 0.90
name: Luminous
description: >
  Dark-first desktop music player UI. Tokens below describe the default
  "Luminous" theme (see src/lib/stores/theme.svelte.ts LUMINOUS_DARK_COLORS)
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
  accent-gold: "#FFB648"
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

# Luminous Design System

## Overview

Luminous is a dense, dark-first desktop music player (Tauri v2 + Svelte 5 Runes). The default aesthetic — codenamed "Luminous Dark" — favors a near-black canvas, muted slate-blue accents, and generous use of soft rounded geometry over hard edges, evoking a calm, focused listening environment rather than a flashy media app.

The UI is built almost entirely from panels: a collapsible **Sidebar**, a **TopNavigation** ribbon, the main content canvas, a multi-tab **RightPanel** (Synced Lyrics, Queue Drawer, Liner Notes), and a persistent **PlayerBar** transport dock. All chrome and content surfaces read colors exclusively from CSS custom properties (`--bg-main`, `--color-accent`, etc., wired through `@theme` in `src/app.css`) rather than literal hex values in components — enabling seamless runtime theme switching and dynamic artwork color extraction.

Theming is **not static**: users can switch between several built-in themes (Luminous, Ruby Red, Nordic Blue, Retro Amber, System light/dark, Dynamic Artwork), derive themes dynamically from playing cover art, or build a custom theme in the in-app Custom Theme Builder (Settings → Themes tab, in `FoldersView.svelte`; the previous standalone `DesignTools.svelte` panel was folded into Settings). Every theme is generated or validated against a **WCAG AA 4.5:1 contrast guarantee** between accent and background (`clampForContrast()` / `generatePaletteFromSeed()` in `src/lib/utils/colorUtils.ts`). This document describes the shipped default theme; treat its *structure and rules* (spacing, radii, elevation approach, contrast discipline) as normative for new UI.

## Colors

- **bg-main (#08090c):** Near-black base canvas behind all content — the quietest, base layer.
- **bg-sidebar (#1c1f29) / bg-playerbar (#191b23):** Slightly lighter "chrome" tones that separate navigation and transport controls from the content canvas without a hard border.
- **accent (#6f7ea9) / accent-hover (#8c98ba):** A muted, desaturated slate blue — the hue used for interactive emphasis (active nav item, play button, primary actions, focus/selection states). Never used decoratively.
- **accent-text (#6f7ea9):** The accent color clamped to WCAG AA 4.5:1 against `bg-main`. This is the *only* accent-derived value allowed on text or icons sitting directly on the main canvas — using raw `accent` for text risks failing contrast on generated or custom themes.
- **accent-contrast (#000000):** Heuristically derived per-theme text/icon color for content placed *on top of* an accent-colored surface (e.g. the glyph inside a filled play button).
- **text-primary (#f1f3f8) / text-secondary (#a6adc4):** Primary reads as near-white for titles and active content; secondary is a desaturated blue-gray for metadata, captions, and inactive labels.
- **border (#2c2f3c):** The sole structural line color, almost always used at reduced opacity (`/40`, `/50`, `/60`) rather than full strength.
- **accent-gold (#FFB648):** Fixed brand color (not theme-swappable), borrowed from the logo's eclipse ring. Reserved for rare one-off highlight moments — a "Featured" badge, a premium marker, a milestone toast. Never used on buttons, active states, or anything recurring.
- **artwork-extracted tokens:** `--color-artwork-primary`, `--color-artwork-accent`, etc., dynamically extracted from current album art when Adaptive Dynamic Cover Tinting is enabled.

## Typography

Single typeface throughout: **Inter** (`src/app.css:51`), falling back to system UI sans. There is no secondary display face — hierarchy comes from size and weight.

- **Display (36px / 700):** Reserved for album/artist hero titles at the top of detail views.
- **Headline (24px–20px / 700–600):** Section headers and dialog titles.
- **Title (18px / 600):** Card and list-group headers.
- **Body (14px / 12px, 400):** The workhorse sizes — the large majority of UI text in Luminous is `body-sm` (12px) for metadata-dense lists (folders, tags, playlists) and `body-md` (14px) for primary row labels and buttons.
- **Label (12px / 600):** Interactive text — buttons, active tab labels, song titles that double as links.
- **Micro badge (9px–11px / 700, tracked uppercase):** Format/quality tags (e.g. FLAC, 24-bit, Hi-Res) rendered as tiny pills — bold and uppercase for legibility.

Weight usage: `medium` (500) for inactive nav items, `semibold` (600) for buttons and clickable labels, `bold` (700) for badges and large numerals, `extrabold` (800) reserved for the single active lyric line in `LyricsView`.

## Layout & Adaptive Information Density

Luminous uses Tailwind's default 4px spacing unit:

- **Tight (4–8px):** Icon gaps, chip padding (`px-3 py-1`).
- **Standard (12–16px):** Button padding (`px-4 py-2`), card gaps.
- **Generous (24–32px):** Section padding inside large panels (`p-6`, `p-8` in `Equalizer.svelte`).

### Adaptive Information Density Matrix

The viewport layout supports three density modes to accommodate different display sizes and user preferences:

1. **Compact Mode**: 32px padding, hidden cover thumbnails in dense track tables, optimized for data-dense browsing.
2. **Balanced Mode (Default)**: 48px padding, standard 40px thumbnails, 14px track labels.
3. **Expanded Mode**: 72px+ detailed layout blocks, enlarged cover art, expanded metadata grid rows.

### Fixed Chrome Architecture

The application uses a fixed viewport shell:
- **Sidebar**: Fixed width, collapsible to icon-only rail mode.
- **TopNavigation**: Fixed 80px header housing navigation buttons, universal search, density toggles, and view options.
- **Content Canvas**: Scrollable central viewport (Personalized Home Hub, Category Explorer, Album Parallax Hero, Artist Hub).
- **RightPanel**: Collapsible side drawer for `.LRC` Synced Lyrics, Queue Drawer (Playing Next / History), and Liner Notes.
- **PlayerBar**: Fixed 80px bottom dock housing media transport, volume, visualizer toggles (FFT / Moodbar / Waveform), and miniplayer detachment controls.

## Elevation & Depth

Luminous is largely **flat**: hierarchy comes from background tone steps (`bg-main` → `bg-sidebar`/`bg-playerbar`) and low-opacity borders:

- `shadow-sm`/`shadow-xs` mark subtle resting state on interactive chrome.
- `shadow-md`/`shadow-lg` mark active/hover elevation, tinted with the accent color (`shadow-brand-accent/20`, `shadow-brand-accent/10`) for a soft accent glow.
- `shadow-xl`/`shadow-2xl` mark modal-like overlays (`TagEditor`, `SmartPlaylistModal`) and hero album art.
- **Glassmorphism surface**: Applied via `.glass-surface` (`backdrop-filter: blur(20px) saturate(180%)`) on all chrome panels (Sidebar, TopNavigation, RightPanel, PlayerBar, Miniplayer). This is no longer System-theme-exclusive — every theme now gets the glass treatment (`themeStore.isGlassTheme` is unconditionally `true`), with the `--glass-*` custom properties recomputed per-theme from that theme's own resolved hex colors.

## Shapes

Rounded corners scale with component visual weight:

- **sm (6px):** Compact icon-toggle buttons.
- **md (8px):** Default for buttons, list rows, cover thumbnails, form inputs.
- **lg (12px):** Elevated cards, folder rows, settings tab strips.
- **xl (16px):** Large containers — hero album art, modal dialogs (`TagEditor`), equalizer panel.
- **full:** Circular controls (play/pause, avatars, miniplayer buttons) and pill-shaped filter chips/tabs.
- **PlayerBar dock (32px / 2rem):** Custom radius reading as a distinct floating transport surface.

## Component Specifications

- **Buttons:** Primary actions use `bg-accent` with `accent-contrast` text, `semibold`, and an accent-tinted shadow glow. Secondary/outline actions use a transparent background with a `border` and hover to `bg-sidebar`. Play controls are always circular (`rounded-full`).
- **Filter Chips:** `rounded-full`, `px-3 py-1`, `body-sm` weight `medium`. Active state uses `bg-border` + `text-primary` + `semibold`.
- **Nav items (Sidebar):** `rounded-lg` expanded, `rounded-xl` collapsed. Active item uses `bg-accent` + `accent-contrast` text + accent glow shadow.
- **Cards (Album/Artist Covers):** `rounded-lg`, half-opacity `border`, hovering to full accent border and accent-tinted glow shadow.
- **Badges (Audio Quality):** Micro-badge typography, `rounded`, low-opacity accent background (`bg-accent/15`) with low-opacity accent border.
- **Modals (`TagEditor`, `SmartPlaylistModal`):** `bg-sidebar`, full-opacity `border`, `rounded-2xl`, `shadow-2xl` heavy overlay surface.

## Do's and Don'ts

- Do read all colors from `--color-brand-*` / `--bg-*` custom properties via Tailwind `brand-*` classes — never hardcode theme hex values in components.
- Do use `accent-text` (contrast-clamped token), not raw `accent`, for text/icons sitting directly on `bg-main`/`bg-sidebar` to guarantee WCAG AA compliance.
- Do tint elevation shadows with the accent color on interactive elements.
- Do reserve `rounded-full` for circular controls and pill chips — avoid using it on rectangular cards or panels.
- Do use full-strength `text-brand-text-secondary` for metadata/captions — never dilute it with opacity modifiers (`/40`–`/90`). An accessibility audit swept these out app-wide because diluted secondary text failed contrast checks; `border` opacity modifiers are unaffected and remain the normal pattern.
- Don't introduce a second typeface; hierarchy comes strictly from size and weight.
- Don't add heavy drop shadows to flat chrome panels beyond the standard glass treatment (now applied to all themes, not just System).
- Do reserve `accent-gold` for rare highlight moments (Featured badge, premium marker, milestone toast) — never on buttons, active states, or anything recurring.
- Don't put a gradient or colored frame on a custom (user-made) playlist — flat/no-frame is what visually marks a playlist as custom.
- Don't round the corners of album artwork or crop it — art is always shown square and in its entirety; only the outer card/frame gets corner radius.

## Playlist Card System

Special playlist categories carry a colored gradient identity, distinct from custom (user-made) playlists, which get no color treatment at all. Full reference: `Luminous Playlist Card System.dc.html`.

### Frame color families (playlists with real album art)

- **Blue** (`#2563EB → #38BDF8`) — Decades.
- **Green** (`#059669 → #34D399`) — Genres.
- **Orange** (`#C2410C → #F59E0B`) — Smart Rule playlists.

The gradient renders as a thin border framing the stacked album art, plus a matching badge pill ("Auto" for Decades/Genres, "✦ Smart" for Smart Rule playlists). All gradients run 135° (top-left to bottom-right).

### Icon tiles (playlists without stable album art)

Favourites, Recently Added, and Queue have no consistent art to frame, so they get a full-bleed gradient tile with a single glyph instead. Each has its own distinct signature gradient, independent of the frame color families above:

- Favourites (heart): `#DB2777 → #F43F5E` (pink/rose)
- Recently Added (clock): `#CA8A04 → #FACC15` (gold/amber)
- Queue (stack): `#4338CA → #7C3AED` (indigo/violet)

### Custom playlists

User-made playlists get no frame and no gradient — plain album art with the standard card border. Gradient is the one visual signal for "the app built this for you"; flat is what marks a playlist as custom.

## Brand & Logo

The mark is an eclipse motif — a black disc with a corona flare, seen edge-on. It exists in two modes: **static** (app icon, social avatar, marketing — a fixed "at rest" pose) and **reactive** (embedded in the running app — an audio-visualizer element driven live by the current track's frequency bands). Full reference: `Luminous Logo System.dc.html`; source: `assets/luminous-mark.svg`.

### Layers (back to front)

1. **Ambient glow** — soft blurred halo. Recolors with the active theme (or extracted album-art color). Driven by **mid** frequencies at runtime (radius/opacity).
2. **Eclipse ring** — thin ring at the disc's edge. Recolors alongside the glow, one step brighter. Driven by **treble** (stroke-width/opacity).
3. **Black silhouette** — the disc itself. Always `#0A0A0D`, fixed regardless of theme or artwork. Never recolors.
4. **Coronal burst** — bright flare at the disc's edge (fixed position, upper-right). Always pure white. Driven by **bass** (radius/opacity).

### Brand colors (fixed, outside the swappable UI theme)

- Ink (silhouette): `#0A0A0D`
- Paper (corona): `#FFFFFF`
- Primary — Indigo: `#626FE8`
- Accent — Gold: `#FFB648`

These are the logo's own identity colors for marketing/social/app-icon-at-rest use — independent from the in-app swappable `accent` token. Inside the running app, glow + ring instead re-target to whichever theme (or extracted artwork palette) is active.

### Wordmark

"Luminous" is set in **Space Grotesk** (600–700 weight) — a geometric sans whose circular counters echo the mark. It is the only place a second typeface appears; the tagline "Music Player" and all other UI text stay in Inter. Approved lockups: icon alone, icon + wordmark (horizontal), icon + wordmark (stacked), icon + wordmark + "Music Player" tagline (larger/marketing use only). Don't rearrange these relative to each other.

### Two artifacts

- `luminous-mark.svg` — static, crisp, no blur or glow filters. Use for app icons, avatars, favicons, and anywhere the mark must render sharp at small sizes.
- `luminous-mark-reactive.svg` — lush, heavier blur/glow baked in (bigger ambient glow, blurred rings, blurred burst halo). Use for hero/marketing placements and as the resting frame of the in-app audio-reactive element.
- Never blur the static mark or crisp up the reactive one — they're distinct assets for distinct contexts, not one file with a toggle.

### Usage rules

- Icon ships mark-only, no container — add a squircle/rounded-square background per-platform only where the OS requires it (e.g. macOS dock).
- Minimum clearspace: ¼ of the mark's diameter on every side.
- Below 24px, drop the glow and ring — render silhouette + burst only.
- Never recolor the black silhouette or white burst; never stretch, skew, or add drop shadows/outlines to the mark.
