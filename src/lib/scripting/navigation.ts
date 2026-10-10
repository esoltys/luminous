import { navigationStore, type ActiveTab, type ActiveSubTab, type SettingsTab, type AutoPlaylistRef } from "../stores/navigation.svelte";
import { collectionStore } from "../stores/collection.svelte";
import type { ScriptNavigationApi, ScriptWaitApi } from "./types";

/**
 * Creates the navigation controller.
 * Encapsulates multi-step store state transitions behind intention-revealing calls
 * and guarantees view settlement.
 */
export function createNavigationController(wait: ScriptWaitApi): ScriptNavigationApi {
  return {
    async to(tab: ActiveTab, subTab?: ActiveSubTab | "auto" | "custom"): Promise<void> {
      collectionStore.searchQuery = "";
      collectionStore.searchResults = [];

      navigationStore.activeTab = tab;
      if (subTab === "auto" || subTab === "custom") {
        navigationStore.playlistsSubTab = subTab;
      } else if (subTab) {
        navigationStore.activeSubTab = subTab;
      }
      navigationStore.selectedAlbumName = null;
      navigationStore.selectedArtistName = null;
      navigationStore.selectedPlaylistId = null;
      navigationStore.selectedAutoPlaylist = null;

      await wait.settled();
    },

    async album(albumName: string, focusSongId?: number): Promise<void> {
      navigationStore.viewAlbum(albumName, focusSongId);
      await wait.settled();
      // Wait until the view has settled on this album
      await wait.forState(() => navigationStore.selectedAlbumName === albumName, 3000);
    },

    async artist(artistName: string): Promise<void> {
      navigationStore.viewArtist(artistName);
      await wait.settled();
      await wait.forState(() => navigationStore.selectedArtistName === artistName, 3000);
    },

    async playlist(target: number | AutoPlaylistRef["kind"]): Promise<void> {
      if (typeof target === "number") {
        navigationStore.viewPlaylist(target);
        await wait.settled();
        await wait.forState(() => navigationStore.selectedPlaylistId === target, 3000);
      } else {
        navigationStore.viewAutoPlaylist({ kind: target });
        await wait.settled();
        await wait.forState(() => navigationStore.selectedAutoPlaylist?.kind === target, 3000);
      }
    },

    async settings(section: SettingsTab = "general"): Promise<void> {
      navigationStore.openSettings(section);
      await wait.settled();
      await wait.forState(
        () => navigationStore.activeTab === "settings" && navigationStore.settingsSubTab === section,
        3000
      );
    },
  };
}
