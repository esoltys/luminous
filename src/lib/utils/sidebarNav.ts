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

/** Left padding that puts a 14px icon on the same vertical line as the main nav icons. */
export const SIDEBAR_SUB_ITEM_INDENT_CLASS = "pl-[calc(var(--sidebar-icon-axis)_-_7px)]";

/**
 * A single-line sub-item row under a section label. `--sidebar-icon-axis` is defined by
 * Sidebar.svelte; the indent is that axis minus half the icon's width.
 */
export function sidebarSubItemClass(active: boolean): string {
  return `w-full flex items-center ${SIDEBAR_SUB_ITEM_INDENT_CLASS} pr-2.5 py-1 rounded-md ${SIDEBAR_NAV_TEXT_CLASS} transition-colors ${
    active ? SIDEBAR_ROW_SELECTED_CLASS : SIDEBAR_ROW_IDLE_CLASS
  }`;
}

/** Layout and typography classes for top-level sidebar buttons (Home, Organize, Stats, footer). */
export function sidebarTopItemLayoutClass(collapsed: boolean): string {
  return collapsed
    ? "justify-center w-10 h-10 rounded-xl p-0"
    : `w-full px-3 py-1.5 rounded-lg ${SIDEBAR_NAV_TEXT_CLASS}`;
}
