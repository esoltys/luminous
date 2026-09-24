/**
 * Remote-source detection (#916). Songs synced from a WebDAV share or an
 * OpenSubsonic server have no local file: their tags can't be written, they
 * can't be opened in Picard, organized on disk, or revealed in a file manager.
 */
import type { SongSource } from "../types";

const REMOTE_SOURCES: ReadonlySet<SongSource> = new Set<SongSource>(["web_dav", "subsonic"]);

/** Matches a WebDAV playback URL (`http(s)://…`) or a `subsonic://{server}/{track}` path. */
const REMOTE_PATH = /^(?:https?|subsonic):\/\//i;

/** True when `path` points at a remote source rather than a local file. */
export function isRemotePath(path: string | null | undefined): boolean {
  return !!path && REMOTE_PATH.test(path);
}

/**
 * True when a song lives on a remote source. Accepts anything with a `source`
 * and/or `path`, checking `source` first and falling back to the path.
 */
export function isRemoteSource(
  song: { source?: SongSource | null; path?: string | null } | null | undefined
): boolean {
  if (!song) return false;
  if (song.source && REMOTE_SOURCES.has(song.source)) return true;
  return isRemotePath(song.path);
}

/** Parses a `subsonic://{serverId}/{trackId}` path; `null` for anything else. */
export function parseSubsonicPath(
  path: string | null | undefined
): { serverId: number; trackId: string } | null {
  const m = path?.match(/^subsonic:\/\/(\d+)\/(.+)$/i);
  if (!m) return null;
  return { serverId: Number(m[1]), trackId: m[2] };
}
