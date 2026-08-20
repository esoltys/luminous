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
  /** Songs with no genre value at all — excluded from allTags/genreGraph
   * entirely (there's nothing to group them by), tracked separately so the
   * Genres tab can still surface them as their own browsable group. */
  noGenreCount = $state(0);
  loaded = $state(false);

  async load() {
    // One combined call — list_all_tags/get_genre_graph each independently
    // rescan every song's genre column backend-side, and this pair is always
    // needed together whenever the Genres tab opens.
    const { tags, graph, no_genre_count } = await invoke<{
      tags: Tag[];
      graph: GenreGroup[];
      no_genre_count: number;
    }>("get_tags_overview");
    this.allTags = tags;
    this.genreGraph = graph;
    this.noGenreCount = no_genre_count;
    this.loaded = true;
  }

  async getSongsWithoutGenre(limit?: number, mode?: QueuePopulationMode): Promise<Song[]> {
    return invoke<Song[]>("get_songs_without_genre", { limit, mode });
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
