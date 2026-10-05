// Shared numeric constants used across multiple components/stores.
// File-local magic numbers that only ever appear in one place stay named
// inline near their usage instead of being centralized here.

/** Estimated width of a floating context menu, used to keep it inside the viewport. */
export const CONTEXT_MENU_WIDTH_PX = 200;

/** Minimum gap kept between a floating menu/modal and the edge of the viewport. */
export const VIEWPORT_EDGE_PADDING_PX = 10;

/**
 * The floating PlayerBar dock (h-20 + bottom-4 inset ≈ 96px) sits on top of
 * page content whenever a song is loaded. Anything that positions itself
 * near the bottom of the viewport (context menus, modals) needs to clamp
 * above it so it isn't hidden underneath.
 */
export const PLAYER_DOCK_CLEARANCE_PX = 96;

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

/** Width below which the right panel auto-hides and playbar spectrum/info/lyrics buttons hide. Tailwind `md`. */
export const BREAKPOINT_RIGHT_PANEL_PX = 768;

/** Width below which the sidebar auto-collapses to its icon rail. Tailwind `lg`. */
export const BREAKPOINT_EXPANDED_PX = 1024;

/** Height below which the app collapses to showing only the PlayerBar (routes/+layout.svelte, collection.svelte.ts). */
export const HEIGHT_BREAKPOINT_MINIMAL_PX = 160;

/** Height below which detail-view hero headers (Playlist/Album/Artist) hide, so their song table gets the space instead (collection.svelte.ts). */
export const HEIGHT_BREAKPOINT_SHORT_PX = 600;

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
