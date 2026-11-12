/**
 * Tempo buckets a Smart Playlist `tempo:` rule can name, slowest first.
 *
 * `token` is what the rule stores and the backend's `tempo:` filter parses;
 * it must match `TEMPO_BUCKETS` in `src-tauri/src/playlist/tempo_buckets.rs`,
 * which also owns the BPM ranges. `labelKey` is the bucket's display-name i18n
 * key, shared with the BPM auto-playlists so the two always read the same.
 */
export const TEMPO_BUCKET_OPTIONS = [
  { token: "slow", labelKey: "playlists.bpmSlow" },
  { token: "down-tempo", labelKey: "playlists.bpmDownTempo" },
  { token: "mid-tempo", labelKey: "playlists.bpmMidTempo" },
  { token: "uptempo", labelKey: "playlists.bpmUptempo" },
  { token: "high-energy", labelKey: "playlists.bpmHighEnergy" },
  { token: "extreme", labelKey: "playlists.bpmExtreme" },
] as const;

/** A tempo rule's stored value if it names a known bucket, else the first bucket. */
export function normalizeTempoToken(value: string): string {
  const token = value.trim().toLowerCase();
  return TEMPO_BUCKET_OPTIONS.some((o) => o.token === token)
    ? token
    : TEMPO_BUCKET_OPTIONS[0].token;
}
