import { invoke } from "@tauri-apps/api/core";
import type { GenreGroup, QueuePopulationMode, Song, Tag } from "../types";

/** Luminous-native song tags (#224) — independent of the embedded `Song.genre`. */
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

  async getSongTags(songId: number): Promise<string[]> {
    return invoke<string[]>("get_song_tags", { songId });
  }

  /** Replaces a song's whole tag list, in order — the first tag is treated
   * as its main category for the Genre browse view. Refreshes the cached
   * tag/genre lists afterward so the browse view reflects the change. */
  async setSongTags(songId: number, tags: string[]) {
    await invoke("set_song_tags", { songId, tagNames: tags });
    await this.load();
  }

  async getSongsByTag(tagName: string, limit?: number, mode?: QueuePopulationMode): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_tag", { tagName, limit, mode });
  }
}

export const tagsStore = new TagsStore();
