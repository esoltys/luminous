/**
 * Album identity (#1752, #1932): tracks belong to the same album release when
 * their composite keys are equal. Mirrors the backend's `album_key`
 * (`src-tauri/src/album_key.rs`), which groups the album grid and keys ratings,
 * profiles and charts — both are checked against `albumKey.fixture.json`.
 *
 * For single-artist releases, the key is `{folded_artist}\u{1f}{folded_album}`.
 * For compilations, the artist portion is `[va]`.
 */

const ALBUM_KEY_SEPARATOR = "\u{1f}";
const COMPILATION_ARTIST_KEY = "[va]";

function foldKey(s: string | null | undefined): string {
  return (s ?? "").trim().toLowerCase();
}

function isVariousArtists(artist: string | null | undefined): boolean {
  const folded = foldKey(artist);
  return (
    folded === "various artists" ||
    folded === "various" ||
    folded === "v/a" ||
    folded === "va" ||
    folded === "v.a." ||
    folded === "v. a." ||
    folded === "[va]"
  );
}

export function albumKey(
  album: string | null | undefined,
  artist?: string | null | undefined,
  isCompilation = false
): string {
  const foldedAlbum = foldKey(album);
  if (!foldedAlbum) return "";

  if (foldedAlbum.includes(ALBUM_KEY_SEPARATOR)) {
    return foldedAlbum;
  }

  if (!artist && !isCompilation) {
    return foldedAlbum;
  }

  const artistKey = isCompilation || isVariousArtists(artist)
    ? COMPILATION_ARTIST_KEY
    : foldKey(artist);

  return `${artistKey}${ALBUM_KEY_SEPARATOR}${foldedAlbum}`;
}

function extractAlbumTitle(keyOrTitle: string | null | undefined): string {
  const s = foldKey(keyOrTitle);
  const sepIdx = s.indexOf(ALBUM_KEY_SEPARATOR);
  return sepIdx >= 0 ? s.slice(sepIdx + 1) : s;
}

function extractAlbumArtist(keyOrTitle: string | null | undefined): string | null {
  const s = foldKey(keyOrTitle);
  const sepIdx = s.indexOf(ALBUM_KEY_SEPARATOR);
  return sepIdx >= 0 ? s.slice(0, sepIdx) : null;
}

/** Whether two album titles or keys name the same album (see `albumKey`). */
export function sameAlbum(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  const keyA = albumKey(a);
  const keyB = albumKey(b);
  if (keyA === keyB) return true;

  const artA = extractAlbumArtist(keyA);
  const artB = extractAlbumArtist(keyB);
  if (artA !== null && artB !== null) {
    return artA === artB && extractAlbumTitle(keyA) === extractAlbumTitle(keyB);
  }
  return extractAlbumTitle(keyA) === extractAlbumTitle(keyB);
}
