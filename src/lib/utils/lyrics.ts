import type { Song } from "../types";

export type LyricsStatus = "none" | "synced" | "plain";

/**
 * Whether a song's lyrics (if any) are LRC time-synced or plain text.
 * Mirrors the convention LyricsView.svelte writes/reads: an explicit
 * "[synced:false]" prefix marks plain text explicitly (e.g. edited by the
 * user or fetched from a plain-text-only source); otherwise the presence of
 * an LRC timestamp tag (e.g. "[01:23.45]") means the lyrics are synced.
 */
export function lyricsStatus(song: Song): LyricsStatus {
  const lyrics = song.lyrics;
  if (!lyrics || lyrics.trim() === "") return "none";
  if (lyrics.startsWith("[synced:false]")) return "plain";
  return /\[\d{1,2}:\d{2}(\.\d{1,2})?\]/.test(lyrics) ? "synced" : "plain";
}
