import type { FileSource } from "../types";

interface FileTagTarget {
  filePath: string;
  source?: FileSource;
  folderPath?: string[];
  folderName?: string;
  fileName?: string;
}

type TagEditorTarget = { kind: "song"; songId: number } | ({ kind: "file" } & FileTagTarget);

interface TagEditorOpenOptions {
  onSaved?: () => void;
}

/**
 * Coordinates the single global `<TagEditor>` host mounted in `+layout.svelte`.
 *
 * Views call `tagEditorStore.open(songId, { onSaved })` or
 * `tagEditorStore.open({ filePath, ... }, { onSaved })` instead of mounting
 * their own `<TagEditor>` instance.
 */
class TagEditorStore {
  target = $state<TagEditorTarget | null>(null);
  onSaved = $state<(() => void) | undefined>(undefined);
  /** Monotonic counter so re-opening for the same target resets editor state. */
  openSeq = $state(0);

  get isOpen(): boolean {
    return this.target !== null;
  }

  open(target: number | FileTagTarget, options?: TagEditorOpenOptions): void {
    this.target =
      typeof target === "number" ? { kind: "song", songId: target } : { kind: "file", ...target };
    this.onSaved = options?.onSaved;
    this.openSeq += 1;
  }

  close(): void {
    this.target = null;
    this.onSaved = undefined;
  }
}

export const tagEditorStore = new TagEditorStore();
