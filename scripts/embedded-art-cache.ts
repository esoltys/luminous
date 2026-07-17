// Extracts embedded cover art (ID3 APIC / FLAC PICTURE / etc.) straight from
// the audio files themselves. The real backend only does this on demand, via
// the get_cover_art_uri command (see src-tauri/src/covermanager.rs) — that
// command doesn't exist in the browser-only mock, so any song whose
// art_automatic/art_manual are both null (the common case for embedded art
// that was never pre-cached to a file) would otherwise show the placeholder
// icon forever.
//
// Extracted images are cached under the OS temp dir — deliberately *not*
// the real app's covers/ cache, so this tooling never writes into your
// actual Luminous app data. Filenames are given an "album-" prefix so
// CoverArt.svelte's `artAutomatic.startsWith("album-")` check treats them
// as a cache-relative filename (`luminous-art://album-mockembed-<id>.jpg`)
// rather than an absolute path — which matters because an absolute POSIX
// path starts with "/", and CoverArt.svelte treats *any* "/"-prefixed
// artAutomatic as an already-servable URL, bypassing the luminous-art
// scheme (and this tooling's route interception) entirely.
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import type { Song } from "../src/lib/types/index";

export const EMBEDDED_ART_CACHE_DIR = path.join(os.tmpdir(), "luminous-mock-embedded-art");
const FILENAME_PREFIX = "album-mockembed-";

const EXTENSION_BY_MIME: Record<string, string> = {
  "image/jpeg": "jpg",
  "image/jpg": "jpg",
  "image/png": "png",
  "image/gif": "gif",
  "image/webp": "webp",
};

/**
 * Returns the luminous-art cache filename (e.g. "album-mockembed-123.jpg")
 * for `song`'s embedded cover art, or undefined if the file has no embedded
 * picture (or can't be read at all — a Windows-recorded path that no longer
 * resolves, corrupt tags, etc). Never throws; a failure here should just
 * mean "no art", not abort the whole mock library load.
 */
export async function resolveEmbeddedArt(song: Song): Promise<string | undefined> {
  if (!song.path || !existsSync(song.path)) return undefined;

  for (const ext of ["jpg", "png"]) {
    const filename = `${FILENAME_PREFIX}${song.id}.${ext}`;
    if (existsSync(path.join(EMBEDDED_ART_CACHE_DIR, filename))) return filename;
  }

  try {
    const { parseFile } = await import("music-metadata");
    const metadata = await parseFile(song.path);
    const picture = metadata.common.picture?.[0];
    if (!picture) return undefined;

    const ext = EXTENSION_BY_MIME[picture.format.toLowerCase()] ?? "jpg";
    const filename = `${FILENAME_PREFIX}${song.id}.${ext}`;
    mkdirSync(EMBEDDED_ART_CACHE_DIR, { recursive: true });
    writeFileSync(path.join(EMBEDDED_ART_CACHE_DIR, filename), picture.data);
    return filename;
  } catch (err) {
    console.warn(`[Mock Library] Could not read embedded art from "${song.path}":`, (err as Error).message);
    return undefined;
  }
}

/** Runs `resolveEmbeddedArt` over `songs` with bounded concurrency, mutating art_automatic in place. */
export async function hydrateEmbeddedArt(songs: Song[], concurrency = 8): Promise<void> {
  const candidates = songs.filter((s) => s.art_embedded && !s.art_automatic && !s.art_manual && s.path);
  if (candidates.length === 0) return;

  let cursor = 0;
  async function worker() {
    while (cursor < candidates.length) {
      const song = candidates[cursor++];
      const filename = await resolveEmbeddedArt(song);
      if (filename) song.art_automatic = filename;
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, candidates.length) }, worker));
}
