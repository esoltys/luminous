import type { FileSource } from "../types";
import { radioStore, stationUuidFromPath } from "./radio.svelte";

interface FileTagTarget {
  filePath: string;
  source?: FileSource;
  folderPath?: string[];
  folderName?: string;
  fileName?: string;
}

interface SongTagTarget {
  id: number;
  path?: string | null;
}

type TagEditorTarget = { kind: "song"; songId: number } | ({ kind: "file" } & FileTagTarget);

interface TagEditorOpenOptions {
  onSaved?: () => void;
}

/**
 * Coordinates the single global `<TagEditor>` host mounted in `+layout.svelte`.
 *
 * Views call `tagEditorStore.open(song, { onSaved })`, `tagEditorStore.open(songId, { onSaved })`,
 * or `tagEditorStore.open({ filePath, ... }, { onSaved })` instead of mounting their own
 * `<TagEditor>` instance. Streaming radio tracks (`radio://...`) are routed to the radio
 * station editor instead of the local-file tag editor.
 */
class TagEditorStore {
  target = $state<TagEditorTarget | null>(null);
  onSaved = $state<(() => void) | undefined>(undefined);
  /** Monotonic counter so re-opening for the same target resets editor state. */
  openSeq = $state(0);

  get isOpen(): boolean {
    return this.target !== null;
  }

  open(target: number | SongTagTarget | FileTagTarget, options?: TagEditorOpenOptions): void {
    if (typeof target === "object" && "id" in target) {
      const stationUuid = stationUuidFromPath(target.path);
      if (stationUuid) {
        this.close();
        void radioStore.openStationEditorByUuid(stationUuid, options?.onSaved);
        return;
      }
      this.target = { kind: "song", songId: target.id };
    } else {
      this.target =
        typeof target === "number" ? { kind: "song", songId: target } : { kind: "file", ...target };
    }
    this.onSaved = options?.onSaved;
    this.openSeq += 1;
  }

  close(): void {
    this.target = null;
    this.onSaved = undefined;
  }
}

export const tagEditorStore = new TagEditorStore();
