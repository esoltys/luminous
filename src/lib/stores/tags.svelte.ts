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

  async getSongsByTag(tagName: string, limit?: number, mode?: QueuePopulationMode): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_tag", { tagName, limit, mode });
  }
}

export const tagsStore = new TagsStore();
