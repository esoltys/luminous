import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";

export type StatsEntityType = "song" | "album" | "artist" | "genre";

/** Personal Stats exclusion set (#130) — preloaded once and kept in sync via
 * `stats-exclusions-changed`, mirroring how `PinnedStore` preloads
 * `get_pinned_items` rather than checking each item's state individually. */
class StatsExclusionsStore {
  private keys = $state<Set<string>>(new Set());

  constructor() {
    this.init();
  }

  private async init() {
    try {
      await listen("stats-exclusions-changed", () => this.refresh());
      await this.refresh();
    } catch (err) {
      console.error("Failed to initialize StatsExclusionsStore:", err);
    }
  }

  async refresh() {
    try {
      const pairs = await invoke<[string, string][]>("get_stats_exclusions");
      this.keys = new Set(
        (pairs ?? []).map(([type, key]) => `${type}:${type === "song" ? key : key.toLowerCase()}`)
      );
    } catch (err) {
      console.error("Failed to load stats exclusions:", err);
    }
  }

  isExcluded(entityType: StatsEntityType, entityKey: string): boolean {
    const k = entityType === "song" ? entityKey : entityKey.toLowerCase();
    return this.keys.has(`${entityType}:${k}`);
  }

  getSongExclusionReason(song: {
    id: number | string;
    album?: string | null;
    artist?: string | null;
    album_artist?: string | null;
  }): "song" | "album" | "artist" | null {
    if (this.isExcluded("song", String(song.id))) {
      return "song";
    }
    if (song.album && this.isExcluded("album", song.album)) {
      return "album";
    }
    const artist = song.album_artist || song.artist;
    if (artist && this.isExcluded("artist", artist)) {
      return "artist";
    }
    return null;
  }

  isSongExcluded(song: {
    id: number | string;
    album?: string | null;
    artist?: string | null;
    album_artist?: string | null;
  }): boolean {
    return this.getSongExclusionReason(song) !== null;
  }

  async setExcluded(entityType: StatsEntityType, entityKey: string, excluded: boolean) {
    await invoke("set_stats_excluded", { entityType, entityKey, excluded });
    await this.refresh();
  }

  async toggle(entityType: StatsEntityType, entityKey: string) {
    await this.setExcluded(entityType, entityKey, !this.isExcluded(entityType, entityKey));
  }

  async toggleWithToast(
    entityType: StatsEntityType,
    entityKey: string,
    displayName?: string
  ) {
    const excluded = !this.isExcluded(entityType, entityKey);
    await this.setExcluded(entityType, entityKey, excluded);
    const fallback =
      entityType === "album" ? i18n.t("collection.unknownAlbum") :
      entityType === "artist" ? i18n.t("collection.unknownArtist") :
      entityType === "song" ? i18n.t("collection.unknownSong") :
      entityKey;
    const name = displayName || (entityKey ? entityKey : fallback);
    const message = excluded
      ? i18n.t("stats.excludedToast", { name })
      : i18n.t("stats.includedToast", { name });
    toastStore.show(message);
  }
}

export const statsExclusionsStore = new StatsExclusionsStore();

export async function toggleStatsExcluded(
  entityType: StatsEntityType,
  entityKey: string,
  displayName?: string
) {
  return statsExclusionsStore.toggleWithToast(entityType, entityKey, displayName);
}

