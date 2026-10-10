import { prefs } from "../stores/prefs.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { organizeStore } from "../stores/organizer.svelte";
import type { ScriptViewApi, ScriptViewSurface, ScriptWaitApi } from "./types";

type ViewMode = "cards" | "rows";

const VIEW_MODE_SETTERS: Record<ScriptViewSurface, (mode: ViewMode) => void> = {
  albums: (m) => prefs.setAlbumsViewMode(m),
  artists: (m) => prefs.setArtistsViewMode(m),
  playlistsAuto: (m) => prefs.setPlaylistsAutoViewMode(m),
  playlistsCustom: (m) => prefs.setPlaylistsCustomViewMode(m),
  genres: (m) => prefs.setGenreCardsViewMode(m),
  pinned: (m) => prefs.setPinnedViewMode(m),
};

/**
 * Creates the view-preferences controller: store-level settings that are
 * otherwise reached by clicking through menus.
 */
export function createViewController(wait: ScriptWaitApi): ScriptViewApi {
  return {
    async setViewMode(surface, mode) {
      VIEW_MODE_SETTERS[surface](mode);
      await wait.settled();
    },

    async setSeekbarMode(mode) {
      if (prefs.seekBarMode !== mode) prefs.toggleSeekBarMode();
      await wait.settled();
    },

    async setColumnVisible(column, visible) {
      const key = column as keyof typeof collectionStore.visibleColumns;
      if (!(key in collectionStore.visibleColumns)) {
        throw new Error(`Unknown song-table column "${column}". Valid: ${Object.keys(collectionStore.visibleColumns).join(", ")}`);
      }
      if (collectionStore.visibleColumns[key] !== visible) collectionStore.toggleColumn(key);
      await wait.settled();
    },

    async setOrganizeTemplate(template) {
      await organizeStore.updateConfig({ preset: "custom", template });
      await wait.settled();
    },

    async openSmartPlaylistBuilder(rules) {
      collectionStore.openSmartBuilder(rules);
      await wait.settled();
    },
  };
}
