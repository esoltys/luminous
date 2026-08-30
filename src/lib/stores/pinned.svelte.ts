import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PinnedItem, PinnedItemType } from "../types";
import { pinnedRefKeyFor } from "../types";

class PinnedStore {
  items = $state<PinnedItem[]>([]);
  private keys = $state<Set<string>>(new Set());
  private libraryChangedDebounce: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    this.init();
  }

  private async init() {
    try {
      await listen("pinned-items-changed", () => this.refresh());
      // Pins resolve against live song/album/artist/playlist data (see
      // get_pinned_items), so a tag edit, rescan, etc. elsewhere in the app
      // should refresh what's shown here too — debounced to match
      // HomeView's own library-changed handling.
      await listen("library-changed", () => {
        clearTimeout(this.libraryChangedDebounce);
        this.libraryChangedDebounce = setTimeout(() => this.refresh(), 500);
      });
      await this.refresh();
    } catch (err) {
      console.error("Failed to initialize PinnedStore:", err);
    }
  }

  async refresh() {
    const items = await invoke<PinnedItem[]>("get_pinned_items");
    this.items = Array.isArray(items) ? items : [];
    this.keys = new Set(this.items.map((item) => `${item.type}:${pinnedRefKeyFor(item)}`));
  }

  isPinned(itemType: PinnedItemType, refKey: string): boolean {
    return this.keys.has(`${itemType}:${refKey}`);
  }

  async pin(itemType: PinnedItemType, refKey: string) {
    await invoke("pin_item", { itemType, refKey });
    await this.refresh();
  }

  async unpin(itemType: PinnedItemType, refKey: string) {
    await invoke("unpin_item", { itemType, refKey });
    await this.refresh();
  }

  async toggle(itemType: PinnedItemType, refKey: string) {
    if (this.isPinned(itemType, refKey)) {
      await this.unpin(itemType, refKey);
    } else {
      await this.pin(itemType, refKey);
    }
  }

  async reorder(order: Array<[PinnedItemType, string]>) {
    const positionOf = new Map(order.map(([type, refKey], idx) => [`${type}:${refKey}`, idx]));
    this.items = [...this.items].sort((a, b) => {
      const posA = positionOf.get(`${a.type}:${pinnedRefKeyFor(a)}`) ?? 0;
      const posB = positionOf.get(`${b.type}:${pinnedRefKeyFor(b)}`) ?? 0;
      return posA - posB;
    });
    await invoke("reorder_pinned_items", { order });
    await this.refresh();
  }
}

export const pinnedStore = new PinnedStore();
