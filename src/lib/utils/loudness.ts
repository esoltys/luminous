import type { Song } from "../types";

/**
 * Plausible integrated-loudness range for a real R128 measurement — mirrors
 * the backend's `PLAUSIBLE_LUFS_RANGE` (src-tauri/src/loudness.rs). Values
 * outside this include the analysis-failure sentinel and should be treated
 * as "no usable measurement" rather than an analyzed track.
 */
function hasUsableAnalysis(song: Song): boolean {
  const lufs = song.ebur128_integrated_loudness_lufs;
  return lufs !== undefined && Number.isFinite(lufs) && lufs >= -70 && lufs <= 0;
}

/**
 * Static loudness-source badge for a song, independent of playback/settings —
 * unlike the player bar's badge (which reflects the currently applied gain),
 * this just reports what data the track *has*: its own R128 analysis, a
 * ReplayGain tag, or neither (no badge).
 */
export function loudnessBadge(song: Song): "R128" | "RG" | null {
  if (hasUsableAnalysis(song)) return "R128";
  if (song.replaygain_track_gain !== undefined || song.replaygain_album_gain !== undefined) {
    return "RG";
  }
  return null;
}
