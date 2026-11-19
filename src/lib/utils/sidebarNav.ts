/**
 * Class strings shared by the expanded sidebar's rows, so Sidebar, PinnedNavList,
 * PinnedNavItem, and SidebarFilesSection share one typographic scale and cannot drift.
 */

/** Primary text style for every navigable row in the expanded sidebar (top-level, pins, sub-items). */
export const SIDEBAR_NAV_TEXT_CLASS = "text-sm font-medium";

/** Secondary caption text style for subtitles, device storage/sync status, and drag-expanded lists. */
export const SIDEBAR_CAPTION_TEXT_CLASS = "text-xs font-normal";

/** A quiet, non-interactive section heading ("Pinned", "Library", "Playlists", "Files"). */
export const SIDEBAR_SECTION_LABEL_CLASS =
  "w-full px-3 pt-2.5 pb-0.5 text-xs font-medium text-brand-text-secondary/70 truncate";

/** Selected/unselected colours of a sub-item row; the selected one matches the main nav buttons. */
export const SIDEBAR_ROW_SELECTED_CLASS =
  "bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20";
export const SIDEBAR_ROW_IDLE_CLASS =
  "text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10";

/** Indented sub-item container with a vertical margin line aligned to the parent icon axis. */
export const SIDEBAR_SUB_TREE_CLASS =
  "pl-[9px] pr-1 py-0.5 space-y-0 border-l-2 border-brand-accent/30 ml-[calc(var(--sidebar-icon-axis)_-_1px)] my-0.5";

/**
 * A single-line sub-item row inside a `SIDEBAR_SUB_TREE_CLASS` container.
 */
export function sidebarSubItemClass(active: boolean): string {
  return `w-full flex items-center px-2.5 py-1 rounded-md ${SIDEBAR_NAV_TEXT_CLASS} transition-colors ${
    active ? SIDEBAR_ROW_SELECTED_CLASS : SIDEBAR_ROW_IDLE_CLASS
  }`;
}

/** Layout and typography classes for top-level sidebar buttons (Home, Organize, Stats, footer). */
export function sidebarTopItemLayoutClass(collapsed: boolean): string {
  return collapsed
    ? "justify-center w-10 h-10 rounded-xl p-0"
    : `w-full px-3 py-1.5 rounded-lg ${SIDEBAR_NAV_TEXT_CLASS}`;
}
