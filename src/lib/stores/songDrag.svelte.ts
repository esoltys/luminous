import type { FileEntry, Song } from "../types";
import { playlistsStore } from "./playlists.svelte";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";

export type SongDropTarget =
  | { kind: "queue" }
  | { kind: "custom-category" }
  | { kind: "playlist"; id: number; name: string };

/**
 * Inspects a DOM element or its ancestors for a song drop target attribute (`data-drop-target`).
 */
export function parseDropTarget(el: HTMLElement | null): SongDropTarget | null {
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

/**
 * Finds the drop target under the given viewport coordinates (x, y).
 * Uses elementsFromPoint first, and falls back to geometric bounding rect matching
 * across all [data-drop-target] elements. This guarantees detection even when pointer
 * capture, CSS backdrop filters, or webview hit-testing limitations obscure targets.
 */
export function findDropTargetAt(x: number, y: number): SongDropTarget | null {
  if (typeof document === "undefined") return null;

  if (typeof document.elementsFromPoint === "function") {
    const elements = document.elementsFromPoint(x, y);
    for (const el of elements) {
      const dropEl = el.closest<HTMLElement>("[data-drop-target]");
      if (dropEl) {
        const parsed = parseDropTarget(dropEl);
        if (parsed) return parsed;
      }
    }
  }

  if (typeof document.querySelectorAll === "function") {
    const targets = Array.from(document.querySelectorAll<HTMLElement>("[data-drop-target]"));
    for (let i = targets.length - 1; i >= 0; i--) {
      const target = targets[i];
      const rect = target.getBoundingClientRect();
      if (
        rect.width > 0 &&
        rect.height > 0 &&
        x >= rect.left &&
        x <= rect.right &&
        y >= rect.top &&
        y <= rect.bottom
      ) {
        const parsed = parseDropTarget(target);
        if (parsed) return parsed;
      }
    }
  }

  return null;
}

/** What a song drag carries, whatever it started from. */
export interface SongDragPayload {
  /** Library song ids to add on drop. */
  songIds: number[];
  /** Ghost text: one item's name, or a count. */
  label: string;
  /** Dragged files that aren't in the library and will be left out (File Browser only). */
  skippedCount: number;
  /** Playlist the songs were dragged out of. Dropping them back onto it would only add
   * duplicates, so that target is never offered. */
  sourcePlaylistId: number | null;
}

/** Payload for files dragged from the File Browser. Folders are ignored; files that aren't in
 * the library are counted so the drop can say they were skipped. */
export function filesPayload(entries: FileEntry[]): SongDragPayload {
  const files = entries.filter((e) => !e.isDir);
  return {
    songIds: files.flatMap((e) => (e.songId !== null ? [e.songId] : [])),
    label: entries.length === 1 ? (entries[0]?.name ?? "") : i18n.plural("files.songCount", entries.length),
    skippedCount: files.filter((e) => e.songId === null).length,
    sourcePlaylistId: null,
  };
}

/** Payload for songs dragged from a song list; `sourcePlaylistId` is the playlist (or Queue)
 * the list shows, if any. */
export function songsPayload(songs: Song[], sourcePlaylistId: number | null = null): SongDragPayload {
  return {
    songIds: songs.map((s) => s.id),
    label: songs.length === 1 ? (songs[0]?.title ?? "") : i18n.plural("files.songCount", songs.length),
    skippedCount: 0,
    sourcePlaylistId,
  };
}

/**
 * The one in-progress drag of songs onto the Queue or a playlist. Drag sources call `start`,
 * `move` and `drop`; the store finds the drop target under the pointer, decides whether it
 * accepts these songs, and performs the add. `SongDragGhost` and the drop targets render from it.
 */
class SongDragStore {
  payload = $state<SongDragPayload | null>(null);
  pointerPos = $state<{ x: number; y: number } | null>(null);
  /** The target under the pointer that would accept a drop right now, or null. */
  dropTarget = $state<SongDropTarget | null>(null);

  get isDragging(): boolean {
    return this.payload !== null;
  }

  start(payload: SongDragPayload, pos: { x: number; y: number }) {
    this.payload = payload;
    this.pointerPos = pos;
    this.dropTarget = null;
  }

  move(pos: { x: number; y: number }) {
    if (!this.payload) return;
    this.pointerPos = pos;
    this.dropTarget = this.accepts(findDropTargetAt(pos.x, pos.y));
  }

  end() {
    this.payload = null;
    this.pointerPos = null;
    this.dropTarget = null;
  }

  /**
   * Ends the drag, adding the songs to the target under `pos` (or the last one seen) and
   * reporting the result in a toast. Returns true when the release landed on the Queue or a
   * playlist that accepts the songs, so the caller can skip its own fallback (such as a reorder).
   */
  async drop(pos?: { x: number; y: number }): Promise<boolean> {
    if (!this.payload) return false;
    if (pos) this.move(pos);
    const target = this.dropTarget;
    const { songIds, skippedCount } = this.payload;
    this.end();

    if (target?.kind !== "queue" && target?.kind !== "playlist") return false;
    if (songIds.length === 0) {
      toastStore.show(i18n.t("files.nothingToPlay"), "info");
      return true;
    }

    try {
      if (target.kind === "queue") {
        await playlistsStore.addSongsToQueue(songIds);
        toastStore.show(i18n.plural("dragDrop.addedSongs", songIds.length), "success");
      } else {
        await playlistsStore.addSongsToPlaylist(target.id, songIds);
        toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: target.name }), "success");
      }
    } catch (err) {
      console.error("Failed to add dragged songs:", err);
      toastStore.show(
        target.kind === "queue"
          ? i18n.t("dragDrop.queueFailed")
          : i18n.t("dragDrop.playlistFailed", { name: target.name }),
        "error"
      );
      return true;
    }
    if (skippedCount > 0) {
      toastStore.show(i18n.plural("files.skipped", skippedCount), "info");
    }
    return true;
  }

  /** Filters out the playlist the songs came from (and the Queue, when they came from it). */
  private accepts(target: SongDropTarget | null): SongDropTarget | null {
    const source = this.payload?.sourcePlaylistId ?? null;
    if (!target || source === null) return target;
    if (target.kind === "playlist" && target.id === source) return null;
    if (target.kind === "queue" && playlistsStore.queuePlaylist?.id === source) return null;
    return target;
  }
}

export const songDrag = new SongDragStore();
