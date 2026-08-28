import { collectionStore } from "./collection.svelte";
import { playlistsStore } from "./playlists.svelte";

export type ActiveTab = "home" | "collection" | "playlists" | "settings" | "lyrics" | "help";
export type ActiveSubTab = "songs" | "albums" | "artists" | "genres";

/** Which grid is shown under the Playlists tab (mirrors `ActiveSubTab` for Collection). */
export type PlaylistsSubTab = "auto" | "custom";

/** An auto-playlist reference (Favourites, Recently Added, genre, decade, BPM,
 * or the genre-less "No Genre" group), for the auto-playlist detail view. */
export interface AutoPlaylistRef {
  kind: "favourites" | "recently_added" | "history" | "genre" | "decade" | "bpm" | "no_genre" | "artist_tag";
  /** For kind "genre": the curated tag's plain name (#548) — a top-level
   * card name or a sub-genre chip name, resolved the same way either way
   * (see `viewGenreTag`). */
  genre?: string;
  artistTag?: string;
  decade?: string;
  /** For kind "bpm": the bucket's dynamic_spec suffix, e.g. "60-90" or the open-ended "150-". */
  bpm?: string;
  /** For kind "genre", "decade", "bpm" or "artist_tag": the materialized (dynamic_enabled) playlist row backing it. */
  playlistId?: number;
  /** For kind "genre", "decade", "bpm" or "artist_tag": when this playlist's songs were last (re)generated. */
  updated?: number;
}

/** A snapshot of "where the user is" for Back/Forward navigation history. */
interface NavigationView {
  activeTab: ActiveTab;
  activeSubTab: ActiveSubTab;
  playlistsSubTab: PlaylistsSubTab;
  selectedArtistName: string | null;
  selectedAlbumName: string | null;
  selectedPlaylistId: number | null;
  selectedAutoPlaylist: AutoPlaylistRef | null;
}

const MAX_HISTORY = 50;

class NavigationStore {
  private _activeTab = $state<ActiveTab>("collection");
  private _activeSubTab = $state<ActiveSubTab>("songs");
  private _playlistsSubTab = $state<PlaylistsSubTab>("custom");

  get activeTab() { return this._activeTab; }
  set activeTab(val) {
    this._activeTab = val;
    if (typeof window !== "undefined") {
      localStorage.setItem("navigation_activeTab", val);
    }
    this.scheduleRecordHistory();
  }

  get activeSubTab() { return this._activeSubTab; }
  set activeSubTab(val) {
    this._activeSubTab = val;
    if (typeof window !== "undefined") {
      localStorage.setItem("navigation_activeSubTab", val);
    }
    this.scheduleRecordHistory();
  }

  get playlistsSubTab() { return this._playlistsSubTab; }
  set playlistsSubTab(val) {
    this._playlistsSubTab = val;
    if (typeof window !== "undefined") {
      localStorage.setItem("navigation_playlistsSubTab", val);
    }
    this.scheduleRecordHistory();
  }

  private _selectedArtistName = $state<string | null>(null);
  private _selectedAlbumName = $state<string | null>(null);
  private _selectedPlaylistId = $state<number | null>(null);
  private _selectedAutoPlaylist = $state<AutoPlaylistRef | null>(null);

  /** Selected real playlist for the Playlist Detail view (rendered inside PlaylistsCollectionView). */
  get selectedPlaylistId() { return this._selectedPlaylistId; }
  set selectedPlaylistId(val) {
    this._selectedPlaylistId = val;
    if (typeof window !== "undefined") {
      if (val !== null) localStorage.setItem("navigation_selectedPlaylistId", String(val));
      else localStorage.removeItem("navigation_selectedPlaylistId");
    }
    this.scheduleRecordHistory();
  }

  /** Selected auto-playlist for the read-only Auto-Playlist Detail view. */
  get selectedAutoPlaylist() { return this._selectedAutoPlaylist; }
  set selectedAutoPlaylist(val) {
    this._selectedAutoPlaylist = val;
    if (typeof window !== "undefined") {
      if (val) localStorage.setItem("navigation_selectedAutoPlaylist", JSON.stringify(val));
      else localStorage.removeItem("navigation_selectedAutoPlaylist");
    }
    this.scheduleRecordHistory();
  }

  /** Selected artist for the Artist Detail view (rendered inside CollectionView). */
  get selectedArtistName() { return this._selectedArtistName; }
  set selectedArtistName(val) {
    this._selectedArtistName = val;
    if (typeof window !== "undefined") {
      if (val) localStorage.setItem("navigation_selectedArtistName", val);
      else localStorage.removeItem("navigation_selectedArtistName");
    }
    this.scheduleRecordHistory();
  }

  /** Selected album for the Album Detail view (rendered inside CollectionView). */
  get selectedAlbumName() { return this._selectedAlbumName; }
  set selectedAlbumName(val) {
    this._selectedAlbumName = val;
    if (typeof window !== "undefined") {
      if (val) localStorage.setItem("navigation_selectedAlbumName", val);
      else localStorage.removeItem("navigation_selectedAlbumName");
    }
    this.scheduleRecordHistory();
  }

  // Back/Forward navigation history. Snapshots are coalesced via a microtask
  // so that a single user action touching several fields in sequence (e.g.
  // viewArtist() setting activeTab/activeSubTab/selectedArtistName) records
  // one history entry instead of one per field write.
  private history = $state<NavigationView[]>([]);
  private historyIndex = $state(-1);
  private isNavigatingHistory = false;
  private historyRecordScheduled = false;

  get canGoBack(): boolean { return this.historyIndex > 0; }
  get canGoForward(): boolean { return this.historyIndex < this.history.length - 1; }

  private snapshotView(): NavigationView {
    return {
      activeTab: this._activeTab,
      activeSubTab: this._activeSubTab,
      playlistsSubTab: this._playlistsSubTab,
      selectedArtistName: this._selectedArtistName,
      selectedAlbumName: this._selectedAlbumName,
      selectedPlaylistId: this._selectedPlaylistId,
      selectedAutoPlaylist: this._selectedAutoPlaylist,
    };
  }

  private scheduleRecordHistory() {
    if (this.isNavigatingHistory || this.historyRecordScheduled) return;
    this.historyRecordScheduled = true;
    queueMicrotask(() => {
      this.historyRecordScheduled = false;
      this.recordHistory();
    });
  }

  private recordHistory() {
    const snap = this.snapshotView();
    const current = this.historyIndex >= 0 ? this.history[this.historyIndex] : undefined;
    if (current && JSON.stringify(current) === JSON.stringify(snap)) return;

    const truncated = this.history.slice(0, this.historyIndex + 1);
    truncated.push(snap);
    if (truncated.length > MAX_HISTORY) truncated.shift();
    this.history = truncated;
    this.historyIndex = truncated.length - 1;
  }

  private applyHistorySnapshot(snap: NavigationView) {
    this.isNavigatingHistory = true;
    this.activeTab = snap.activeTab;
    this.activeSubTab = snap.activeSubTab;
    this.playlistsSubTab = snap.playlistsSubTab;
    this.selectedArtistName = snap.selectedArtistName;
    this.selectedAlbumName = snap.selectedAlbumName;
    this.selectedPlaylistId = snap.selectedPlaylistId;
    this.selectedAutoPlaylist = snap.selectedAutoPlaylist;
    if (snap.selectedPlaylistId !== null) {
      playlistsStore.selectPlaylist(snap.selectedPlaylistId);
    }
    this.isNavigatingHistory = false;
  }

  goBack() {
    if (!this.canGoBack) return;
    this.historyIndex--;
    this.applyHistorySnapshot(this.history[this.historyIndex]);
  }

  goForward() {
    if (!this.canGoForward) return;
    this.historyIndex++;
    this.applyHistorySnapshot(this.history[this.historyIndex]);
  }

  constructor() {
    if (typeof window !== "undefined") {
      const savedTab = localStorage.getItem("navigation_activeTab");
      if (savedTab) this._activeTab = savedTab as ActiveTab;

      const savedSubTab = localStorage.getItem("navigation_activeSubTab");
      if (savedSubTab) this._activeSubTab = savedSubTab as ActiveSubTab;

      const savedPlaylistsSubTab = localStorage.getItem("navigation_playlistsSubTab");
      if (savedPlaylistsSubTab) this._playlistsSubTab = savedPlaylistsSubTab as PlaylistsSubTab;

      // Restore the last-viewed Album/Artist detail view (mutually
      // exclusive — CollectionView prefers the album when both are set).
      const savedAlbum = localStorage.getItem("navigation_selectedAlbumName");
      if (savedAlbum) this._selectedAlbumName = savedAlbum;

      const savedArtist = localStorage.getItem("navigation_selectedArtistName");
      if (savedArtist) this._selectedArtistName = savedArtist;

      // Restore the last-viewed Playlist/Auto-Playlist detail view (mutually
      // exclusive — PlaylistsCollectionView prefers the real playlist when both are set).
      const savedPlaylistId = localStorage.getItem("navigation_selectedPlaylistId");
      if (savedPlaylistId) this._selectedPlaylistId = parseInt(savedPlaylistId, 10);

      const savedAutoPlaylist = localStorage.getItem("navigation_selectedAutoPlaylist");
      if (savedAutoPlaylist) {
        try {
          this._selectedAutoPlaylist = JSON.parse(savedAutoPlaylist) as AutoPlaylistRef;
        } catch (e) {
          console.error("Failed to parse saved selectedAutoPlaylist:", e);
        }
      }
    }

    // Seed history with the restored (or default) view so Back/Forward
    // have a starting point instead of an empty stack on boot.
    this.recordHistory();
  }

  /** Opens a genre/tag's auto-playlist detail view (#548) — every Genres-tab
   * card/chip click and every "browse this genre" entry point elsewhere in
   * the app (e.g. a GenreChips chip on a song row) route through here.
   * Resolves the curated tag's materialized playlist row if one exists
   * (`dynamic_spec === "tag:"+tag`); a tag below the auto-playlist song
   * threshold has none yet, so `playlistId` stays undefined and
   * AutoPlaylistDetailView falls back to a direct curated-hierarchy query. */
  viewGenreTag(tag: string) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.selectedArtistName = null;
    this.selectedAlbumName = null;
    const playlist = playlistsStore.playlists.find(
      (p) => p.dynamic_enabled && p.dynamic_spec === `tag:${tag}`
    );
    this.viewAutoPlaylist({
      kind: "genre",
      genre: tag,
      playlistId: playlist?.id,
      updated: playlist?.updated,
    });
  }

  viewArtist(name: string) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.selectedAlbumName = null;
    this.activeTab = "collection";
    this.activeSubTab = "artists";
    this.selectedArtistName = name;
  }

  viewAlbum(name: string) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.selectedArtistName = null;
    this.activeTab = "collection";
    this.activeSubTab = "albums";
    this.selectedAlbumName = name;
  }

  viewPlaylist(id: number) {
    this.activeTab = "playlists";
    this.playlistsSubTab = "custom";
    this.selectedAutoPlaylist = null;
    this.selectedPlaylistId = id;
  }

  viewAutoPlaylist(ref: AutoPlaylistRef) {
    this.activeTab = "playlists";
    this.playlistsSubTab = "auto";
    this.selectedPlaylistId = null;
    this.selectedAutoPlaylist = ref;
  }
}

export const navigationStore = new NavigationStore();
