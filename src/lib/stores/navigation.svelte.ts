import { collectionStore } from "./collection.svelte";
import { playlistsStore } from "./playlists.svelte";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";
import { sameAlbum, albumKey as computeAlbumKey } from "$lib/utils/albumKey";
import type { FileNavigationTarget, FileSource } from "../types";

export type ActiveTab = "home" | "collection" | "playlists" | "files" | "settings" | "lyrics" | "stats" | "organize" | "help";
export type ActiveSubTab = "songs" | "albums" | "artists" | "genres";
const SETTINGS_TABS = ["general", "system", "sources", "integrations", "themes", "equalizer", "about"] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];

/** Which grid is shown under the Playlists tab (mirrors `ActiveSubTab` for Collection). */
type PlaylistsSubTab = "auto" | "custom";

/** An auto-playlist reference (Favourites, Recently Added, genre, decade, BPM,
 * Missing Metadata, or the genre-less "No Genre" group), for the auto-playlist
 * detail view. */
export interface AutoPlaylistRef {
  kind:
    | "favourites"
    | "recently_added"
    | "most_played"
    | "history"
    | "genre"
    | "decade"
    | "bpm"
    | "no_genre"
    | "artist_tag"
    | "missing_metadata"
    | "missing_musicbrainz"
    | "daypart"
    | "language";
  /** For kind "genre": the curated tag's plain name (#548) — a top-level
   * card name or a sub-genre chip name, resolved the same way either way
   * (see `viewGenreTag`). */
  genre?: string;
  artistTag?: string;
  decade?: string;
  language?: string;
  /** For kind "bpm": the bucket's dynamic_spec suffix, e.g. "60-90" or the open-ended "150-". */
  bpm?: string;
  /** For kind "genre", "decade", "bpm", "artist_tag", or "language": the materialized (dynamic_enabled) playlist row backing it. */
  playlistId?: number;
  /** For kind "genre", "decade", "bpm", "artist_tag", or "language": when this playlist's songs were last (re)generated. */
  updated?: number;
}

/** Where the Files view is: a folder of a source, as folder names from its root. */
interface FilesLocation {
  source: FileSource;
  path: string[];
  /** The entry selected on arriving at `path`, by listed name (View in Files,
   * #1883). Dropped by any move to another folder; later clicks don't update it. */
  selectedName?: string;
  /** On a device: its Files or Sync tab (#839). Files when absent. */
  deviceTab?: DeviceTab;
}

export type DeviceTab = "files" | "sync";

/** A snapshot of "where the user is" for Back/Forward navigation history. */
interface NavigationView {
  activeTab: ActiveTab;
  activeSubTab: ActiveSubTab;
  playlistsSubTab: PlaylistsSubTab;
  selectedArtistName: string | null;
  selectedAlbumName: string | null;
  selectedAlbumKey: string | null;
  selectedPlaylistId: number | null;
  selectedAutoPlaylist: AutoPlaylistRef | null;
  filesLocation: FilesLocation | null;
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

  private _settingsSubTab = $state<SettingsTab>(
    (typeof window !== "undefined" && (localStorage.getItem("navigation_settingsSubTab") as SettingsTab)) || "general"
  );
  get settingsSubTab() { return this._settingsSubTab; }
  set settingsSubTab(val: SettingsTab) {
    this._settingsSubTab = val;
    if (typeof window !== "undefined") {
      localStorage.setItem("navigation_settingsSubTab", val);
    }
  }

  /**
   * Applies the Settings tab saved by the previous session. Called once at startup,
   * before any navigation can happen, so a later `openSettings(tab)` is always an
   * explicit request that nothing overrides. Unknown values are ignored; "folders" is
   * the legacy name of "sources".
   */
  restoreSettingsSubTab(saved: string | undefined) {
    if (saved === "folders") saved = "sources";
    if ((SETTINGS_TABS as readonly string[]).includes(saved as string)) {
      this.settingsSubTab = saved as SettingsTab;
    }
  }

  private _helpSection = $state<string | null>(null);
  get helpSection() { return this._helpSection; }
  set helpSection(val: string | null) { this._helpSection = val; }

  openHelp(section?: string) {
    this._helpSection = section ?? null;
    this.activeTab = "help";
  }

  openSettings(tab: SettingsTab = "general") {
    this.settingsSubTab = tab;
    this.activeTab = "settings";
  }

  private _selectedArtistName = $state<string | null>(null);
  private _selectedAlbumName = $state<string | null>(null);
  private _selectedAlbumKey = $state<string | null>(null);
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
    if (val !== null && playlistsStore.activePlaylistId !== val) {
      playlistsStore.selectPlaylist(val);
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
    if (val === null) {
      this._selectedAlbumKey = null;
      if (typeof window !== "undefined") {
        localStorage.removeItem("navigation_selectedAlbumKey");
      }
    }
    if (typeof window !== "undefined") {
      if (val) localStorage.setItem("navigation_selectedAlbumName", val);
      else localStorage.removeItem("navigation_selectedAlbumName");
    }
    this.scheduleRecordHistory();
  }

  /** Selected album's composite key (effective_artist\u{1f}folded_album) (#1932). */
  get selectedAlbumKey() { return this._selectedAlbumKey; }
  set selectedAlbumKey(val) {
    this._selectedAlbumKey = val;
    if (typeof window !== "undefined") {
      if (val) localStorage.setItem("navigation_selectedAlbumKey", val);
      else localStorage.removeItem("navigation_selectedAlbumKey");
    }
    this.scheduleRecordHistory();
  }

  private _filesLocation = $state<FilesLocation | null>(null);

  /** The folder the Files view shows; null is the Files overview. */
  get filesLocation() { return this._filesLocation; }
  set filesLocation(val) {
    this._filesLocation = val;
    if (typeof window !== "undefined") {
      if (val) localStorage.setItem("navigation_filesLocation", JSON.stringify(val));
      else localStorage.removeItem("navigation_filesLocation");
    }
    this.scheduleRecordHistory();
  }

  /** Opens the Files view at a folder of `source`, or the overview without one. */
  viewFiles(source?: FileSource, path: string[] = []) {
    this.activeTab = "files";
    this.filesLocation = source ? { source, path } : null;
  }

  /**
   * Opens the Files view at a song's or album's place (from `locate_song_in_files`
   * / `locate_album_in_files`), with its file selected and marked as just arrived
   * at. When the target moved and this is only the nearest folder still there,
   * says so in a toast.
   */
  openInFiles(target: FileNavigationTarget) {
    const { source, path, selectedName, exact } = target;
    this.activeTab = "files";
    this.filesLocation = selectedName ? { source, path, selectedName } : { source, path };
    this.pendingFilesArrival = selectedName;
    if (!exact) toastStore.show(i18n.t("files.movedShowingNearest"), "info");
  }

  /** Opens a device's view at `tab`, keeping the storage and folder it was last at. */
  viewDevice(id: string, tab: DeviceTab = "files") {
    const here = this._filesLocation;
    const same = here?.source.kind === "device" && here.source.id === id;
    this.activeTab = "files";
    this.filesLocation = same ? { ...here, deviceTab: tab } : { source: { kind: "device", id }, path: [], deviceTab: tab };
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
      selectedAlbumKey: this._selectedAlbumKey,
      selectedPlaylistId: this._selectedPlaylistId,
      selectedAutoPlaylist: this._selectedAutoPlaylist,
      filesLocation: this._filesLocation,
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
    this.selectedAlbumKey = snap.selectedAlbumKey ?? null;
    this.selectedAlbumName = snap.selectedAlbumName;
    this.selectedPlaylistId = snap.selectedPlaylistId;
    this.selectedAutoPlaylist = snap.selectedAutoPlaylist;
    this.filesLocation = snap.filesLocation;
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

      const savedAlbumKey = localStorage.getItem("navigation_selectedAlbumKey");
      if (savedAlbumKey) this._selectedAlbumKey = savedAlbumKey;

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

      const savedFilesLocation = localStorage.getItem("navigation_filesLocation");
      if (savedFilesLocation) {
        try {
          this._filesLocation = JSON.parse(savedFilesLocation) as FilesLocation;
        } catch (e) {
          console.error("Failed to parse saved filesLocation:", e);
        }
      }
    }

    // Seed history with the restored (or default) view so Back/Forward
    // have a starting point instead of an empty stack on boot.
    this.recordHistory();
  }

  /**
   * Reconciles restored or current navigation state against loaded collection
   * and playlist data, dropping stale targets (e.g. an album, artist, or playlist
   * restored from localStorage that does not exist in the current collection).
   */
  reconcile() {
    if (collectionStore.statsLoaded && !collectionStore.isScanning) {
      if (this._selectedAlbumKey !== null) {
        const albumExists = collectionStore.albums.some((a) => a.album_key === this._selectedAlbumKey);
        if (!albumExists) {
          this.selectedAlbumKey = null;
          this.selectedAlbumName = null;
        }
      } else if (this._selectedAlbumName !== null) {
        const albumExists = collectionStore.albums.some((a) => sameAlbum(a.album, this._selectedAlbumName));
        if (!albumExists) {
          this.selectedAlbumName = null;
        }
      }
      if (this._selectedArtistName !== null) {
        const artistExists = collectionStore.artists.some(
          (a) => a.name?.toLowerCase() === this._selectedArtistName?.toLowerCase()
        );
        if (!artistExists) {
          this.selectedArtistName = null;
        }
      }
    }
    if (playlistsStore.playlists.length > 0 && this._selectedPlaylistId !== null) {
      const playlistExists = playlistsStore.playlists.some((p) => p.id === this._selectedPlaylistId);
      if (!playlistExists) {
        this.selectedPlaylistId = null;
      }
    }
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

  /** Opens an artist tag's auto-playlist detail view — the browsable-only
   * counterpart to `viewGenreTag` for curated artist tags (#962/#956
   * follow-up). Resolves the tag's materialized playlist row if one exists
   * (`dynamic_spec === "artisttag:"+tag`); below the auto-playlist song
   * threshold it has none yet, so `playlistId` stays undefined and
   * AutoPlaylistDetailView falls back to a direct `get_songs_by_artist_tag`
   * query. */
  viewArtistTag(tag: string) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.selectedArtistName = null;
    this.selectedAlbumName = null;
    const playlist = playlistsStore.playlists.find(
      (p) => p.dynamic_enabled && p.dynamic_spec === `artisttag:${tag}`
    );
    this.viewAutoPlaylist({
      kind: "artist_tag",
      artistTag: tag,
      playlistId: playlist?.id,
      updated: playlist?.updated,
    });
  }

  /** Opens a language's auto-playlist detail view. Resolves the language's
   * materialized playlist row if one exists (`dynamic_spec === "language:"+code`);
   * below the auto-playlist song threshold it has none yet, so `playlistId`
   * stays undefined and AutoPlaylistDetailView falls back to a direct
   * `get_songs_by_language` query. */
  viewLanguage(code: string) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.selectedArtistName = null;
    this.selectedAlbumName = null;
    const playlist = playlistsStore.playlists.find(
      (p) => p.dynamic_enabled && p.dynamic_spec === `language:${code}`
    );
    this.viewAutoPlaylist({
      kind: "language",
      language: code,
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

  /** Opens an album's detail view. Pass `focusSongId` (e.g. a song picked
   * from search) to have AlbumDetailView select that track and scroll it into
   * view once the album's tracks load (#1280). Pass `targetAlbumKey` to target an
   * exact release when multiple albums share a title (#1932). */
  viewAlbum(name: string, focusSongId?: number, targetAlbumKey?: string) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.selectedArtistName = null;
    this.activeTab = "collection";
    this.activeSubTab = "albums";

    let resolvedKey = targetAlbumKey;
    if (!resolvedKey && focusSongId !== undefined) {
      const song = collectionStore.songs.find((s) => s.id === focusSongId);
      if (song) {
        resolvedKey = computeAlbumKey(song.album, song.album_artist || song.artist, song.compilation);
      }
    }

    const match = resolvedKey
      ? collectionStore.albums.find((a) => a.album_key === resolvedKey)
      : collectionStore.albums.find((a) => sameAlbum(a.album, name));

    // A track's own tag may spell the title differently from the album grid
    // (#1752); open the grid's spelling so the header matches the card.
    this.selectedAlbumName = match?.album ?? name;
    this.selectedAlbumKey = resolvedKey ?? match?.album_key ?? null;
    this.pendingFocusSongId = focusSongId ?? null;
  }

  viewPlaylist(id: number) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.activeTab = "playlists";
    this.playlistsSubTab = "custom";
    this.selectedAutoPlaylist = null;
    this.selectedPlaylistId = id;
    playlistsStore.selectPlaylist(id);
  }

  viewAutoPlaylist(ref: AutoPlaylistRef) {
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    this.activeTab = "playlists";
    this.playlistsSubTab = "auto";
    this.selectedPlaylistId = null;
    this.selectedAutoPlaylist = ref;
  }

  /** One-shot signal (not persisted) telling the playlist detail view to scroll
   * the currently playing song into view once it renders — set when the
   * playbar's Queue button navigates there, so the user lands on their place
   * in the queue instead of the top. Consumed and cleared by the view. */
  pendingScrollToCurrentSong = $state(false);

  requestScrollToCurrentSong() {
    this.pendingScrollToCurrentSong = true;
  }

  /** One-shot signal (not persisted, not part of Back/Forward history) naming
   * the song AlbumDetailView should select and scroll to once it has loaded
   * `selectedAlbumName`'s tracks. Set by `viewAlbum`; consumed and cleared by
   * the view whether or not the song is on that album. */
  pendingFocusSongId = $state<number | null>(null);

  /** One-shot signal (not persisted, not part of Back/Forward history) naming
   * the Files entry `openInFiles` just arrived at, so the browser can mark it.
   * Consumed and cleared by FileBrowser; history and restart re-select the
   * entry through `filesLocation.selectedName` without marking it again. */
  pendingFilesArrival = $state<string | null>(null);
}

export const navigationStore = new NavigationStore();
