import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PinnedItem, PinnedItemType } from "../types";
import { pinnedRefKeyFor } from "../types";
import { hasPinnedContent, getNavigablePins, type NavigablePin } from "../utils/pinnedNav";

class PinnedStore {
  items = $state<PinnedItem[]>([]);
  private keys = $state<Set<string>>(new Set());
  private libraryChangedDebounce: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    this.init();
  }

  get visibleItems(): PinnedItem[] {
    return this.items.filter(hasPinnedContent);
  }

  get navigableItems(): NavigablePin[] {
    return getNavigablePins(this.items);
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

  /**
   * Reorders items within the visible subset, preserving the relative
   * positions of any hidden (empty) pins in persistent storage.
   */
  async reorderVisible(fromIndex: number, toIndex: number): Promise<void> {
    const visible = this.visibleItems;
    if (
      fromIndex === toIndex ||
      fromIndex < 0 ||
      fromIndex >= visible.length ||
      toIndex < 0 ||
      toIndex >= visible.length
    ) {
      return;
    }

    const moved = visible[fromIndex];
    const target = visible[toIndex];
    const items = this.items.filter((item) => item !== moved);
    const targetPos = items.indexOf(target);
    items.splice(toIndex > fromIndex ? targetPos + 1 : targetPos, 0, moved);

    const order: Array<[PinnedItemType, string]> = items.map((item) => [
      item.type,
      pinnedRefKeyFor(item),
    ]);
    await this.reorder(order);
  }
}

export const pinnedStore = new PinnedStore();

