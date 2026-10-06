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

const MAX_FILENAME_CHARS = 24;
const QUOTED_URL = /'(https?:\/\/[^'\s]+)'/gi;

/**
 * Shortens each single-quoted `http(s)://` URL in an error message to
 * `origin/…filename`, with a long filename ellipsized in the middle so its
 * extension stays visible. A full playback URL is one unbreakable token that
 * overflows a toast.
 */
export function shortenQuotedUrls(text: string): string {
  return text.replace(QUOTED_URL, (_match, url: string) => {
    const withoutQuery = url.split(/[?#]/)[0];
    const originEnd = withoutQuery.indexOf("/", withoutQuery.indexOf("//") + 2);
    if (originEnd === -1) return `'${withoutQuery}'`;
    const origin = withoutQuery.slice(0, originEnd);
    const segments = withoutQuery.slice(originEnd + 1).split("/").filter(Boolean);
    let name = segments.pop() ?? "";
    if (name.length > MAX_FILENAME_CHARS) {
      name = `${name.slice(0, MAX_FILENAME_CHARS - 12)}…${name.slice(-11)}`;
    }
    return `'${origin}/${segments.length > 0 ? "…" : ""}${name}'`;
  });
}
