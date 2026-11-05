import type { FileEntry } from "../types";
import { playlistsStore } from "./playlists.svelte";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";

export type FileDropTarget =
  | { kind: "queue" }
  | { kind: "custom-category" }
  | { kind: "playlist"; id: number; name: string };

/**
 * Inspects a DOM element or its ancestors for a valid file drop target attribute (`data-drop-target`).
 */
export function parseDropTarget(el: HTMLElement | null): FileDropTarget | null {
  const target = el?.closest<HTMLElement>("[data-drop-target]");
  if (!target) return null;
  const kind = target.dataset.dropTarget;
  if (kind === "queue") {
    return { kind: "queue" };
  }
  if (kind === "custom-category") {
    return { kind: "custom-category" };
  }
  if (kind === "playlist") {
    const id = Number(target.dataset.playlistId);
    const name = target.dataset.playlistName ?? "";
    if (!Number.isNaN(id) && name) {
      return { kind: "playlist", id, name };
    }
  }
  return null;
}

class FileDragStore {
  draggedEntries = $state<FileEntry[]>([]);
  isDragging = $state(false);
  pointerPos = $state<{ x: number; y: number } | null>(null);
  dropTarget = $state<FileDropTarget | null>(null);

  /** Song IDs of all dragged files that are already indexed in the library. */
  librarySongIds = $derived(
    this.draggedEntries.flatMap((e) => (!e.isDir && e.songId !== null ? [e.songId] : []))
  );

  /** Number of dragged files that are not indexed in the library. */
  nonLibraryCount = $derived(
    this.draggedEntries.filter((e) => !e.isDir && e.songId === null).length
  );

  /** Whether the drag payload has at least one valid library song. */
  hasPlayableSongs = $derived(this.librarySongIds.length > 0);

  start(entries: FileEntry[], pos?: { x: number; y: number }) {
    this.draggedEntries = entries;
    this.isDragging = true;
    this.pointerPos = pos ?? null;
    this.dropTarget = null;
  }

  updatePointer(pos: { x: number; y: number }, target: FileDropTarget | null) {
    this.pointerPos = pos;
    this.dropTarget = target;
  }

  end() {
    this.draggedEntries = [];
    this.isDragging = false;
    this.pointerPos = null;
    this.dropTarget = null;
  }

  /**
   * Internal helper executing a drop action across valid library songs,
   * showing feedback for empty batches, success, and skipped non-library files.
   */
  private async executeDrop(
    action: (songIds: number[]) => Promise<void>,
    onSuccess: (count: number) => void,
    onFailureKey: string
  ): Promise<void> {
    if (!this.isDragging) return;
    const songIds = this.librarySongIds;
    const skippedCount = this.nonLibraryCount;
    this.end();

    if (songIds.length === 0) {
      toastStore.show(i18n.t("files.nothingToPlay"), "info");
      return;
    }

    try {
      await action(songIds);
      onSuccess(songIds.length);
      if (skippedCount > 0) {
        toastStore.show(i18n.plural("files.skipped", skippedCount), "info");
      }
    } catch (err) {
      console.error("Failed to execute file drop:", err);
      toastStore.show(i18n.t(onFailureKey), "error");
    }
  }

  /**
   * Appends dragged library songs to the playback queue and ends the drag.
   * Reports success and skipped non-library files via toast.
   */
  async dropToQueue(): Promise<void> {
    await this.executeDrop(
      (songIds) => playlistsStore.addSongsToQueue(songIds),
      (count) => toastStore.show(i18n.plural("dragDrop.addedSongs", count), "success"),
      "files.queueFailed"
    );
  }

  /**
   * Adds dragged library songs to the specified custom playlist and ends the drag.
   * Reports success and skipped non-library files via toast.
   */
  async dropToPlaylist(playlist: { id: number; name: string }): Promise<void> {
    await this.executeDrop(
      (songIds) => playlistsStore.addSongsToPlaylist(playlist.id, songIds),
      () => toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: playlist.name }), "success"),
      "files.playlistFailed"
    );
  }
}

export const fileDrag = new FileDragStore();
