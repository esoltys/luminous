/**
 * Album identity (#1752): two album titles are the same album when their keys
 * are equal. Mirrors the backend's `album_key` (`src-tauri/src/album_key.rs`),
 * which groups the album grid and keys ratings, profiles and charts — both are
 * checked against `albumKey.fixture.json`, so compare album names with this,
 * never with `===` or an ad-hoc `toLowerCase()`.
 */
export function albumKey(album: string | null | undefined): string {
  return (album ?? "").trim().toLowerCase();
}

/** Whether two album titles name the same album (see `albumKey`). */
export function sameAlbum(a: string | null | undefined, b: string | null | undefined): boolean {
  return albumKey(a) === albumKey(b);
}
