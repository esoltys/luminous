import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { GenreGroup, QueuePopulationMode, Song, Tag, TagGroup } from "../types";

/**
 * Genre/tag browsing (#224) plus the persisted Genres curation hierarchy
 * (#545). Reads the existing `Song.genre` column — genre *is* the tag
 * system, there's no separate Luminous-native tag store. Editing a song's
 * genre (and therefore its tags) goes through the existing full tag editor
 * (`save_song_tags`) or the new merge/delete/reparent commands below, all of
 * which already write both the embedded file tag and the DB column; this
 * store only refreshes afterward.
 */
class TagsStore {
  allTags = $state<Tag[]>([]);
  genreGraph = $state<GenreGroup[]>([]);
  /** Songs with no genre value at all — excluded from allTags/genreGraph
   * entirely (there's nothing to group them by), tracked separately so the
   * Genres tab can still surface them as their own browsable group. */
  noGenreCount = $state(0);
  loaded = $state(false);

  /** The persisted Genres curation hierarchy — one card per primary genre. */
  hierarchy = $state<TagGroup[]>([]);

  /** Call from the Genres tab's onMount (and unlisten on unmount, same as
   * its existing "library-changed" listener) to keep `hierarchy` in sync
   * with backend reconciliation (new/vanished tag names) without the user
   * having to manually refresh. */
  async listenForHierarchyChanges(): Promise<() => void> {
    return listen("tags-changed", () => {
      this.loadHierarchy();
    });
  }

  async loadHierarchy() {
    this.hierarchy = await invoke<TagGroup[]>("get_tag_hierarchy");
  }

  async setGroupColor(name: string, colorIndex: number) {
    await invoke("set_tag_group_color", { name, colorIndex });
    await this.loadHierarchy();
  }

  async reparentTag(tagName: string, newGroupName: string) {
    await invoke("reparent_tag", { tagName, newGroupName });
    await this.loadHierarchy();
  }

  async promoteTag(tagName: string) {
    await invoke("promote_tag", { tagName });
    await this.loadHierarchy();
  }

  /** Demotes a top-level card to a sub-genre chip under another card
   * (dragging a card's header onto another card) — distinct from
   * reparentTag, which only moves an existing chip and leaves any
   * separately-existing card of the same name alone. */
  async demoteGroupToChild(tagName: string, newGroupName: string) {
    await invoke("demote_group_to_child", { tagName, newGroupName });
    await this.loadHierarchy();
  }

  async reorderTagInGroup(tagName: string, newIndex: number) {
    await invoke("reorder_tag_in_group", { tagName, newIndex });
    await this.loadHierarchy();
  }

  /** Merges `from` into `into` (both the embedded file tag and the DB, across
   * every affected song) and returns how many songs were updated. */
  async mergeTags(from: string, into: string): Promise<number> {
    const count = await invoke<number>("merge_tags", { from, into });
    await Promise.all([this.load(), this.loadHierarchy()]);
    return count;
  }

  /** Deletes `names` from every affected song and returns how many were
   * updated. */
  async deleteTags(names: string[]): Promise<number> {
    const count = await invoke<number>("delete_tags", { names });
    await Promise.all([this.load(), this.loadHierarchy()]);
    return count;
  }

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

  /** Curated-hierarchy lookup (#548) — dispatches to a top-level card's
   * self+children match or a leaf/chip's exact any-position match depending
   * on what `name` currently is in the hierarchy. Used as the sub-threshold
   * fallback (no backing playlist row yet) for a genre auto-playlist's
   * click-through. */
  async getSongsByCuratedTag(tagName: string, limit?: number, mode?: QueuePopulationMode): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_curated_tag", { tagName, limit, mode });
  }
}

export const tagsStore = new TagsStore();
