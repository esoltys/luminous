import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type StatsEntityType = "song" | "album" | "artist" | "genre";

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
      this.keys = new Set((pairs ?? []).map(([type, key]) => `${type}:${key}`));
    } catch (err) {
      console.error("Failed to load stats exclusions:", err);
    }
  }

  isExcluded(entityType: StatsEntityType, entityKey: string): boolean {
    return this.keys.has(`${entityType}:${entityKey}`);
  }

  async setExcluded(entityType: StatsEntityType, entityKey: string, excluded: boolean) {
    await invoke("set_stats_excluded", { entityType, entityKey, excluded });
    await this.refresh();
  }

  async toggle(entityType: StatsEntityType, entityKey: string) {
    await this.setExcluded(entityType, entityKey, !this.isExcluded(entityType, entityKey));
  }
}

export const statsExclusionsStore = new StatsExclusionsStore();
