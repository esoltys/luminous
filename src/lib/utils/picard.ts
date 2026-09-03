import { invoke } from "@tauri-apps/api/core";
import { toastStore } from "../stores/toast.svelte";

/**
 * Launches MusicBrainz Picard with the given songs' files (#367) — a
 * one-way handoff; Luminous's scanner/watcher picks up any tag changes
 * later. Surfaces the backend's "Picard not found" message (or any other
 * launch failure) as a toast rather than throwing into the caller.
 */
export async function openInPicard(songIds: number[]): Promise<void> {
  if (songIds.length === 0) return;
  try {
    await invoke("open_in_picard", { songIds });
  } catch (err) {
    toastStore.show(String(err), "error");
  }
}
