import { invoke } from "@tauri-apps/api/core";
import { collectionStore } from "../stores/collection.svelte";
import { MixedField, type FieldEdit } from "./mixedField.svelte";
import type { Song } from "../types";

/** The tags the album and batch editors edit, as one track has them. */
export type SharedTagValues = Pick<
  Song,
  | "album"
  | "albumsort"
  | "album_artist"
  | "album_artist_sort"
  | "genre"
  | "genresort"
  | "year"
  | "disc"
  | "disc_subtitle"
  | "compilation"
>;

/** What `save_album_tags` / `save_batch_file_tags` take as `edit`, mirroring
    the backend's `SharedTagEdit`. */
export interface SharedTagEdit {
  album: FieldEdit<string>;
  albumsort: FieldEdit<string>;
  albumArtist: FieldEdit<string>;
  albumArtistSort: FieldEdit<string>;
  genre: FieldEdit<string>;
  genresort: FieldEdit<string>;
  year: FieldEdit<number>;
  disc: FieldEdit<number>;
  discSubtitle: FieldEdit<string>;
  compilation: FieldEdit<boolean>;
}

/**
 * The album and batch editors' fields for a set of tracks that may disagree:
 * an album's discs, or a selection spanning albums. Each field shows the value
 * every track shares, or reads as mixed, and only fields the user edits are
 * written — the rest keep each track's own value.
 */
export class SharedTagFields {
  album = new MixedField<string>();
  albumsort = new MixedField<string>();
  albumArtist = new MixedField<string>();
  albumArtistSort = new MixedField<string>();
  genre = new MixedField<string>();
  genresort = new MixedField<string>();
  year = new MixedField<number>();
  disc = new MixedField<number>();
  discSubtitle = new MixedField<string>();
  compilation = new MixedField<boolean>();

  /** Loads the tracks' current tags. A track with no album artist shows its
      track artist, the album artist it's filed under. */
  reset(tracks: (SharedTagValues & Pick<Song, "artist">)[]) {
    this.album.reset(tracks.map((t) => t.album));
    this.albumsort.reset(tracks.map((t) => t.albumsort));
    this.albumArtist.reset(tracks.map((t) => t.album_artist || t.artist));
    this.albumArtistSort.reset(tracks.map((t) => t.album_artist_sort));
    this.genre.reset(tracks.map((t) => t.genre));
    this.genresort.reset(tracks.map((t) => t.genresort));
    this.year.reset(tracks.map((t) => t.year));
    this.disc.reset(tracks.map((t) => t.disc));
    this.discSubtitle.reset(tracks.map((t) => t.disc_subtitle));
    this.compilation.reset(tracks.map((t) => t.compilation));
  }

  get edit(): SharedTagEdit {
    return {
      album: this.album.edit,
      albumsort: this.albumsort.edit,
      albumArtist: this.albumArtist.edit,
      albumArtistSort: this.albumArtistSort.edit,
      genre: this.genre.edit,
      genresort: this.genresort.edit,
      year: this.year.edit,
      disc: this.disc.edit,
      discSubtitle: this.discSubtitle.edit,
      compilation: this.compilation.edit,
    };
  }
}

/** The library songs `songIds` name, from the loaded library when it has
    them all, otherwise from the backend. */
export async function librarySongs(songIds: number[]): Promise<Song[]> {
  const inStore = songIds.map((id) => collectionStore.songs.find((s) => s.id === id));
  if (inStore.every((s) => s)) return inStore as Song[];
  return (await invoke<Song[]>("get_songs_by_ids", { songIds })) ?? [];
}
