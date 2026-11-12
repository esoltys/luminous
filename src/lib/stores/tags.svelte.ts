import { invoke } from "@tauri-apps/api/core";
import { createEventScope } from "../utils/eventScope";
import type {
  GenreAlias,
  GenreGroup,
  LanguageSummary,
  QueuePopulationMode,
  Song,
  Tag,
  TagGroup,
  TagGroupChild,
} from "../types";
export type { TagGroup, TagGroupChild };

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
  private events = createEventScope(import.meta.hot);
  allTags = $state<Tag[]>([]);
  genreGraph = $state<GenreGroup[]>([]);
  languages = $state<LanguageSummary[]>([]);
  private languagesLoadStarted = false;
  /** Songs with no genre value at all — excluded from allTags/genreGraph
   * entirely (there's nothing to group them by), tracked separately so the
   * Genres tab can still surface them as their own browsable group. */
  noGenreCount = $state(0);
  loaded = $state(false);

  /** The persisted Genres curation hierarchy — one card per primary genre. */
  hierarchy = $state<TagGroup[]>([]);
  private hierarchyLoadStarted = false;

  /** Artist tags (curated, DB-only, `artist_profiles.tags` — #962/#956/#1105).
   * Now supporting a single-layer hierarchy with group cards and child chips. */
  artistTags = $state<Tag[]>([]);
  private artistTagsLoadStarted = false;

  artistHierarchy = $state<TagGroup[]>([]);
  private artistHierarchyLoadStarted = false;

  async loadArtistTags() {
    const result = await invoke<Tag[]>("get_artist_tags_overview");
    this.artistTags = Array.isArray(result) ? result : [];
  }

  ensureArtistTagsLoaded() {
    if (this.artistTagsLoadStarted) return;
    this.artistTagsLoadStarted = true;
    this.loadArtistTags().catch((e) => console.error("Failed to load artist tags:", e));
  }

  async listenForArtistHierarchyChanges(): Promise<() => void> {
    return this.events.listen("artist-tags-changed", () => {
      this.loadArtistHierarchy();
      this.loadArtistTags();
    });
  }

  async loadArtistHierarchy() {
    const result = await invoke<TagGroup[]>("get_artist_tag_hierarchy");
    this.artistHierarchy = Array.isArray(result) ? result : [];
  }

  ensureArtistHierarchyLoaded() {
    if (this.artistHierarchyLoadStarted) return;
    this.artistHierarchyLoadStarted = true;
    this.loadArtistHierarchy().catch((e) =>
      console.error("Failed to load artist tag hierarchy:", e),
    );
  }

  async setArtistGroupColor(name: string, colorIndex: number) {
    await invoke("set_artist_group_color", { name, colorIndex });
    await this.loadArtistHierarchy();
  }

  async reparentArtistTag(tagName: string, newGroupName: string) {
    await invoke("reparent_artist_tag", { tagName, newGroupName });
    await Promise.all([this.loadArtistHierarchy(), this.loadArtistTags()]);
  }

  async promoteArtistTag(tagName: string) {
    await invoke("promote_artist_tag", { tagName });
    await Promise.all([this.loadArtistHierarchy(), this.loadArtistTags()]);
  }

  async demoteArtistGroupToChild(tagName: string, newGroupName: string) {
    await invoke("demote_artist_group_to_child", { tagName, newGroupName });
    await Promise.all([this.loadArtistHierarchy(), this.loadArtistTags()]);
  }

  async reorderArtistTagInGroup(tagName: string, newIndex: number) {
    await invoke("reorder_artist_tag_in_group", { tagName, newIndex });
    await this.loadArtistHierarchy();
  }

  async createArtistTagGroup(name: string) {
    await invoke("create_artist_tag_group", { name });
    await Promise.all([this.loadArtistHierarchy(), this.loadArtistTags()]);
  }

  async mergeArtistTags(from: string, into: string): Promise<number> {
    const count = await invoke<number>("merge_artist_tags", { from, into });
    await Promise.all([this.loadArtistHierarchy(), this.loadArtistTags()]);
    return count;
  }

  async deleteArtistTags(names: string[]): Promise<number> {
    const count = await invoke<number>("delete_artist_tags", { names });
    await Promise.all([this.loadArtistHierarchy(), this.loadArtistTags()]);
    return count;
  }

  /** Call from the Genres tab's onMount (and unlisten on unmount, same as
   * its existing "library-changed" listener) to keep `hierarchy` in sync
   * with backend reconciliation (new/vanished tag names) without the user
   * having to manually refresh. */
  async listenForHierarchyChanges(): Promise<() => void> {
    return this.events.listen("tags-changed", () => {
      this.loadHierarchy();
      this.loadGenreAliases().catch((e) => console.error("Failed to load genre aliases:", e));
    });
  }

  /** Non-destructive genre aliases (#1942). The backend resolves them in
   * every genre read; this list only feeds the management dialog. */
  genreAliases = $state<GenreAlias[]>([]);

  /** Where the alias dialog is open: `null` closed, otherwise the alias
   * spelling to prefill ("" from the toolbar). One mount point in
   * GenreBrowseView serves both the context menu and the toolbar. */
  aliasDialogPrefill = $state<string | null>(null);

  openAliasDialog(alias = "") {
    this.aliasDialogPrefill = alias;
  }

  closeAliasDialog() {
    this.aliasDialogPrefill = null;
  }

  /** Aliases that roll up into `genre`, case-insensitively (empty for an alias). */
  aliasesOf(genre: string): GenreAlias[] {
    const key = genre.toLowerCase();
    return this.genreAliases.filter((a) => a.canonical.toLowerCase() === key);
  }

  /** The canonical genre `name` is read as, or `undefined` when it isn't an alias. */
  canonicalOf(name: string): string | undefined {
    const key = name.toLowerCase();
    return this.genreAliases.find((a) => a.alias.toLowerCase() === key)?.canonical;
  }

  /** Loads `genreAliases` and keeps them fresh until the returned function is
   * called, for a view that shows aliases without owning the Genres tab. */
  watchGenreAliases(): () => void {
    const reload = () =>
      this.loadGenreAliases().catch((e) => console.error("Failed to load genre aliases:", e));
    reload();
    let unlisten: (() => void) | undefined;
    let stopped = false;
    this.events.listen("tags-changed", reload).then((fn) => {
      if (stopped) fn();
      else unlisten = fn;
    });
    return () => {
      stopped = true;
      unlisten?.();
    };
  }

  async loadGenreAliases() {
    const result = await invoke<GenreAlias[]>("get_genre_aliases");
    this.genreAliases = Array.isArray(result) ? result : [];
  }

  async setGenreAlias(alias: string, canonical: string) {
    await invoke("set_genre_alias", { alias, canonical });
    await Promise.all([this.loadGenreAliases(), this.load()]);
  }

  async removeGenreAlias(alias: string) {
    await invoke("remove_genre_alias", { alias });
    await Promise.all([this.loadGenreAliases(), this.load()]);
  }

  async loadHierarchy() {
    const result = await invoke<TagGroup[]>("get_tag_hierarchy");
    this.hierarchy = Array.isArray(result) ? result : [];
  }

  /** Kick off the initial hierarchy load at most once per session — genre
   * chips across many widely-reused components (AlbumCard, PlaylistView,
   * ArtistDetailView, ...) call this from an $effect on every mount, and an
   * empty result (no genres curated yet) is a legitimate steady state, not
   * a signal to keep retrying; live updates already flow through
   * `listenForHierarchyChanges`'s "tags-changed" listener instead. */
  ensureHierarchyLoaded() {
    if (this.hierarchyLoadStarted) return;
    this.hierarchyLoadStarted = true;
    this.loadHierarchy().catch((e) => console.error("Failed to load tag hierarchy:", e));
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

  async loadLanguages() {
    const result = await invoke<LanguageSummary[]>("get_languages_overview");
    this.languages = Array.isArray(result) ? result : [];
  }

  ensureLanguagesLoaded() {
    if (this.languagesLoadStarted) return;
    this.languagesLoadStarted = true;
    this.loadLanguages().catch((e) => console.error("Failed to load languages:", e));
  }

  async load() {
    // One combined call — list_all_tags/get_genre_graph each independently
    // rescan every song's genre column backend-side, and this pair is always
    // needed together whenever the Genres tab opens.
    const [{ tags, graph, no_genre_count }] = await Promise.all([
      invoke<{
        tags: Tag[];
        graph: GenreGroup[];
        no_genre_count: number;
      }>("get_tags_overview"),
      this.loadLanguages().catch((e) => {
        console.error("Failed to load languages:", e);
      }),
    ]);
    this.allTags = tags;
    this.genreGraph = graph;
    this.noGenreCount = no_genre_count;
    this.loaded = true;
  }

  async getSongsWithoutGenre(limit?: number, mode?: QueuePopulationMode): Promise<Song[]> {
    return invoke<Song[]>("get_songs_without_genre", { limit, mode });
  }

  async getSongsByLanguage(
    language: string,
    limit?: number,
    mode?: QueuePopulationMode,
  ): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_language", { language, limit, mode });
  }

  /** Any-position match — used by the flat Tags view, where a tag click is a
   * plain search over every song carrying that value anywhere. */
  async getSongsByTag(
    tagName: string,
    limit?: number,
    mode?: QueuePopulationMode,
  ): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_tag", { tagName, limit, mode });
  }

  /** Curated-hierarchy lookup (#548) — dispatches to a top-level card's
   * self+children match or a leaf/chip's exact any-position match depending
   * on what `name` currently is in the hierarchy. Used as the sub-threshold
   * fallback (no backing playlist row yet) for a genre auto-playlist's
   * click-through. */
  async getSongsByCuratedTag(
    tagName: string,
    limit?: number,
    mode?: QueuePopulationMode,
  ): Promise<Song[]> {
    return invoke<Song[]>("get_songs_by_curated_tag", { tagName, limit, mode });
  }

  /** Stops listening to backend events. */
  dispose() {
    this.events.dispose();
  }
}

export const tagsStore = new TagsStore();
