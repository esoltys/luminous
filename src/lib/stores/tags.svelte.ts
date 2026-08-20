import { invoke } from "@tauri-apps/api/core";
import type { GenreGroup, QueuePopulationMode, Song, Tag } from "../types";

/**
 * Genre/tag browsing (#224). Reads the existing `Song.genre` column — genre
 * *is* the tag system, there's no separate Luminous-native tag store.
 * Editing a song's genre (and therefore its tags) goes through the existing
 * full tag editor (`save_song_tags`), which already writes both the embedded
 * file tag and the DB column; this store only refreshes afterward.
 */
class TagsStore {
  allTags = $state<Tag[]>([]);
  genreGraph = $state<GenreGroup[]>([]);
  loaded = $state(false);

  async load() {
    const [allTags, genreGraph] = await Promise.all([
      invoke<Tag[]>("list_all_tags"),
      invoke<GenreGroup[]>("get_genre_graph"),
    ]);
    this.allTags = allTags;
    this.genreGraph = genreGraph;
    this.loaded = true;
  }

  /** Any-position match — used by the flat Tags view, where a tag click is a
   * plain search over every song carrying that value anywhere. */
  async getSongsByTag(tagName: string, limit?: number, mode?: QueuePopulationMode): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_tag", { tagName, limit, mode });
  }

  /** Strict main-tag match — used when drilling into a root of the Genre
   * view's hierarchy, so a song that's been reordered to have a different
   * main tag no longer shows up here just because it still carries this
   * value as a subgenre. */
  async getSongsByMainTag(tagName: string, limit?: number, mode?: QueuePopulationMode): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_main_tag", { tagName, limit, mode });
  }

  /** Exact root/child edge match — used when drilling into a specific child
   * under a specific root in the Genre view, so a tag shared as a child
   * under multiple roots only shows songs for *this* relationship. */
  async getSongsByGenreEdge(
    rootTag: string,
    childTag: string,
    limit?: number,
    mode?: QueuePopulationMode
  ): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_genre_edge", { rootTag, childTag, limit, mode });
  }
}

export const tagsStore = new TagsStore();
