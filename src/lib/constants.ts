// Shared numeric constants used across multiple components/stores.
// File-local magic numbers that only ever appear in one place stay named
// inline near their usage instead of being centralized here.

/** Estimated width of a floating context menu, used to keep it inside the viewport. */
export const CONTEXT_MENU_WIDTH_PX = 200;

/** Default width of a context-menu flyout (ContextMenuSubmenu). */
export const CONTEXT_SUBMENU_WIDTH_PX = 208;

/**
 * Attribute marking a portaled flyout as part of its parent context menu, so
 * the menu's outside-click check doesn't dismiss it when the flyout is clicked.
 */
export const CONTEXT_MENU_LAYER_ATTR = "data-context-menu-layer";

/** Minimum gap kept between a floating menu/modal and the edge of the viewport. */
export const VIEWPORT_EDGE_PADDING_PX = 10;

/*
 * The floating PlayerBar dock's geometry, defined once. It sits on top of page
 * content whenever a song is loaded.
 *
 * - Height, inset, and max width are mirrored by the dock's `h-20` / `max-w-[1200px]`
 *   (PlayerBar.svelte) and `bottom-4` (routes/+layout.svelte) classes; playerDock.test.ts
 *   keeps them equal.
 * - Floating menus and modals clamp above PLAYER_DOCK_CLEARANCE_PX.
 * - Scrolling views pad their bottom by PLAYER_DOCK_CONTENT_RESERVE_PX (the clearance
 *   plus a gap) through the `pb-dock` utility, which reads the `--dock-content-inset`
 *   variable the layout sets on <main>. Never hand-write `pb-28`/`bottom-28` for this (use `pb-dock` / `bottom-dock`).
 *   Portaled UI that can't inherit the variable (ColumnSelector) imports the constant.
 *
 * Deliberately not covered: RightPanel's `mb-24` and the immersive face's `pb-32`,
 * which size against different geometry.
 */
export const PLAYER_DOCK_HEIGHT_PX = 80;
export const PLAYER_DOCK_INSET_PX = 16;
export const PLAYER_DOCK_MAX_WIDTH_PX = 1200;
const PLAYER_DOCK_CONTENT_GAP_PX = 16;
export const PLAYER_DOCK_CLEARANCE_PX = PLAYER_DOCK_HEIGHT_PX + PLAYER_DOCK_INSET_PX;
export const PLAYER_DOCK_CONTENT_RESERVE_PX = PLAYER_DOCK_CLEARANCE_PX + PLAYER_DOCK_CONTENT_GAP_PX;

/**
 * Returns true when the centered floating PlayerBar dock extends far enough left
 * to overlap a left-anchored panel of `leftWidth` pixels.
 */
export function doesPlayerDockCoverLeft(viewportWidth: number, leftWidth: number): boolean {
  const playbarLeft = Math.max(
    PLAYER_DOCK_INSET_PX,
    (viewportWidth - PLAYER_DOCK_MAX_WIDTH_PX) / 2,
  );
  return playbarLeft < leftWidth;
}

/** Sidebar width bounds (routes/+layout.svelte resize handle, Sidebar.svelte collapse threshold). */
export const SIDEBAR_MIN_WIDTH_PX = 180;
export const SIDEBAR_MAX_WIDTH_PX = 400;
export const SIDEBAR_COLLAPSED_WIDTH_PX = 64;

/** Right panel width bounds (routes/+layout.svelte resize handle). */
export const RIGHT_PANEL_MIN_WIDTH_PX = 220;
export const RIGHT_PANEL_MAX_WIDTH_PX = 480;

/** Step size used by the keyboard-accessible resize handles for the sidebar/right panel. */
export const PANEL_RESIZE_STEP_PX = 10;

/*
 * Window-size tiers (issue #1154). Width tiers are named by what they decide, and
 * their values mirror the --breakpoint-* tokens in src/app.css so the Tailwind
 * sm:/md:/lg: prefixes and these constants can never disagree
 * (breakpoints.test.ts enforces it). Use these for app-shell decisions only;
 * anything that depends on the space a component actually gets (which moves with the
 * sidebar and right panel) should use a container query instead.
 *
 *   compact   < 640    compact playbar, forced Immersive Mode
 *   medium    640-1023 full playbar, sidebar on its icon rail
 *   expanded  >= 1024  full sidebar
 * with a sub-step at 768 where the right panel and playbar extras appear.
 */

/** Narrow step inside the compact tier (Tailwind `xs`): Previous button and immersive cover size appear. */
export const BREAKPOINT_NARROW_PX = 420;

/** Width at which the layout leaves "compact": below it Immersive Mode force-engages and the PlayerBar goes Compact. Tailwind `sm`. */
export const BREAKPOINT_MEDIUM_PX = 640;

/** Width below which the right panel auto-hides and the PlayerBar drops Shuffle/Repeat and the Lyrics/Song info menu items. Tailwind `md`. */
export const BREAKPOINT_RIGHT_PANEL_PX = 768;

/** Width below which the sidebar auto-collapses to its icon rail. Tailwind `lg`. */
export const BREAKPOINT_EXPANDED_PX = 1024;

/**
 * Width below which the PlayerBar drops its live spectrum. The right-hand cluster (spectrum, Queue,
 * volume, more) needs about 280px but gets a third of the dock, which is about (width - 140) / 3,
 * so under roughly 980px the spectrum would overlap the seek bar; 1100 leaves margin.
 */
export const BREAKPOINT_DOCK_SPECTRUM_PX = 1100;

/** Height below which the app collapses to showing only the PlayerBar (routes/+layout.svelte, collection.svelte.ts). */
export const HEIGHT_BREAKPOINT_MINIMAL_PX = 160;

/** Height below which detail-view hero headers (Album/Artist) and collection headers compact, so their song table gets the space instead (windowLayoutStore.isDetailHeaderCollapsed). */
export const HEIGHT_BREAKPOINT_SHORT_PX = 600;

/**
 * Height below which the playlist views' hero headers (PlaylistView, AutoPlaylistDetailView)
 * shrink to their actions. Higher than {@link HEIGHT_BREAKPOINT_SHORT_PX} because those headers
 * never scroll away: only the song table scrolls (#2273), and a full ~210px header leaves the
 * table about 4 rows at 650px.
 */
export const HEIGHT_BREAKPOINT_PINNED_HEADER_PX = 760;

/**
 * Per-cover offset/scale/opacity step used to fan out a stack of album
 * covers into a 3D-ish pile, for the "left" stacking direction shared by
 * CoverStack.svelte and the playlist header's inline cover stack.
 */
export const COVER_STACK_OFFSET_X_PX = -18;
export const COVER_STACK_OFFSET_Y_PX = -10;
export const COVER_STACK_ROTATION_DEG = -5;
export const COVER_STACK_SCALE_STEP = 0.05;
export const COVER_STACK_OPACITY_STEP = 0.07;

/** Number of matches shown per category in the search-suggestions dropdown. */
export const MAX_SEARCH_SUGGESTIONS_PER_CATEGORY = 3;

/** Number of entries kept in the recent-searches history. */
export const MAX_RECENT_SEARCHES = 10;

/** Lightness step used when nudging a color's HSL lightness to reach a target contrast ratio. */
export const LIGHTNESS_STEP = 0.02;

/** How long a toast notification stays visible before auto-dismissing. */
export const TOAST_DURATION_MS = 4000;

/**
 * CoverStack container-query thresholds (px of the card's own width) at which the 4th, 5th and
 * 6th fanned covers appear. Mirrored by the `@container (min-width: …)` rules in CoverStack.svelte
 * (CSS can't import them); breakpoints.test.ts keeps them equal.
 */
export const COVER_STACK_FOURTH_COVER_PX = 150;
export const COVER_STACK_FIFTH_COVER_PX = 180;
export const COVER_STACK_SIXTH_COVER_PX = 210;
