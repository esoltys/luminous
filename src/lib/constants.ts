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

/** Window width below which the sidebar auto-collapses to its icon rail and the right panel auto-closes (routes/+layout.svelte, collection.svelte.ts). */
export const MEDIUM_BREAKPOINT_WIDTH_PX = 1024;

/** Window width below which Immersive Mode force-engages, since sidebars no longer fit (routes/+layout.svelte, collection.svelte.ts). */
export const SMALL_BREAKPOINT_WIDTH_PX = 640;

/** Window height below which the app collapses to showing only the PlayerBar (routes/+layout.svelte, collection.svelte.ts). */
export const PLAYBAR_ONLY_HEIGHT_BREAKPOINT_PX = 360;

/** Window height below which detail-view hero headers (Playlist/Album/Artist) hide, so their song table gets the space instead (collection.svelte.ts). */
export const DETAIL_HEADER_COLLAPSE_HEIGHT_PX = 600;

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
