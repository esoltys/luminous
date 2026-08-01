import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { i18n } from "./i18n.svelte";
import type {
  Song,
  MusicDirectory,
  LibraryStats,
  ScanProgress,
  AlbumItem,
  ArtistItem,
  RecentSearchItem,
  QueuePopulationMode,
} from "../types";
import { applySongStats, type SongStatsPayload } from "../utils/stats";
import { playlistsStore } from "./playlists.svelte";
import { toastStore } from "./toast.svelte";
import { MAX_RECENT_SEARCHES } from "../constants";

export type ActiveTab = "home" | "collection" | "playlists" | "settings" | "lyrics" | "help";
export type ActiveSubTab = "songs" | "albums" | "artists";

/** Which grid is shown under the Playlists tab (mirrors `ActiveSubTab` for Collection). */
export type PlaylistsSubTab = "auto" | "custom";

export interface VisibleColumns {
  // Core columns (on by default)
  track: boolean;
  title: boolean;
  artist: boolean;
  album: boolean;
  // Optional metatag columns
  album_artist: boolean;
  bitdepth: boolean;
  bitrate: boolean;
  bpm: boolean;
  channels: boolean;
  composer: boolean;
  filesize: boolean;
  format: boolean;
  genre: boolean;
  grouping: boolean;
  initial_key: boolean;
  path: boolean;
  samplerate: boolean;
  year: boolean;
  // Luminous-derived columns
  actions: boolean;
  added: boolean;
  duration: boolean;
  lastplayed: boolean;
  playcount: boolean;
  rating: boolean;
  skipcount: boolean;
}

/** An auto-playlist reference (Favourites, Recently Added, genre, or decade), for the auto-playlist detail view. */
export interface AutoPlaylistRef {
  kind: "favourites" | "recently_added" | "genre" | "decade";
  genre?: string;
  decade?: string;
  /** For kind "genre" or "decade": the materialized (dynamic_enabled) playlist row backing it. */
  playlistId?: number;
  /** For kind "genre" or "decade": when this playlist's songs were last (re)generated. */
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

/** Song-count milestones that trigger Milestone-tier celebrations. */
const MILESTONE_THRESHOLDS = [100, 500, 1000, 2500, 5000, 10000];

class CollectionStore {
  directories = $state<MusicDirectory[]>([]);
  stats = $state<LibraryStats>({
    total_songs: 0,
    total_artists: 0,
    total_albums: 0,
    total_duration_nanosec: 0,
    total_filesize_bytes: 0,
  });
  /** False until the first refreshStats() resolves — `stats.total_songs` starts at 0
   *  before that, so code gating on "library is empty" must wait for this to avoid
   *  treating "not loaded yet" as "confirmed empty" (e.g. flashing the sidebar/tab
   *  to an empty-library state on every launch before real stats arrive). */
  statsLoaded = $state<boolean>(false);
  isScanning = $state<boolean>(false);
  scanProgress = $state<ScanProgress | null>(null);

  /** Celebration moment states (issue #182) — consumed by Toast and layout. */
  isFirstLaunch = $state<boolean>(false);
  justAddedFirstFolder = $state<boolean>(false);
  milestoneReached = $state<number | null>(null);

  // Cached collections
  songs = $state<Song[]>([]);
  albums = $state<AlbumItem[]>([]);
  artists = $state<ArtistItem[]>([]);
  searchResults = $state<Song[]>([]);
  searchQuery = $state<string>("");
  searchLoading = $state<boolean>(false);
  recentSearches = $state<RecentSearchItem[]>([]);

  visibleColumns = $state<VisibleColumns>(
    (() => {
      const defaultCols: VisibleColumns = {
        // Core columns on by default
        track: true,
        title: true,
        artist: true,
        album: true,
        // Optional metatag columns
        album_artist: false,
        bitdepth: false,
        bitrate: false,
        bpm: false,
        channels: false,
        composer: false,
        filesize: false,
        format: true,
        genre: false,
        grouping: false,
        initial_key: false,
        path: false,
        samplerate: false,
        year: true,
        // Luminous-derived columns
        actions: true,
        added: false,
        duration: true,
        lastplayed: false,
        playcount: false,
        rating: true,
        skipcount: false,
      };
      if (typeof window !== "undefined") {
        const saved = localStorage.getItem("luminous_visible_columns");
        if (saved) {
          try {
            return { ...defaultCols, ...JSON.parse(saved) };
          } catch (e) {
            console.error("Failed to parse visible columns:", e);
          }
        }
      }
      return defaultCols;
    })()
  );

  toggleColumn(column: keyof VisibleColumns) {
    this.visibleColumns[column] = !this.visibleColumns[column];
    if (typeof window !== "undefined") {
      localStorage.setItem("luminous_visible_columns", JSON.stringify(this.visibleColumns));
    }
  }

  isSmartBuilderOpen = $state<boolean>(false);
  smartBuilderRules = $state<Array<{ field: string; op: string; value: string }>>([]);
  smartBuilderEditing = $state<{
    id: number;
    name: string;
    autoPlay: boolean;
    populationMode?: QueuePopulationMode;
  } | null>(null);

  openSmartBuilder(
    rules?: Array<{ field: string; op: string; value: string }>,
    editing?: { id: number; name: string; autoPlay: boolean; populationMode?: QueuePopulationMode }
  ) {
    this.smartBuilderRules = rules || [];
    this.smartBuilderEditing = editing ?? null;
    this.isSmartBuilderOpen = true;
    this.activeTab = "playlists";
    this.playlistsSubTab = "custom";
  }

  closeSmartBuilder() {
    this.isSmartBuilderOpen = false;
    this.smartBuilderRules = [];
    this.smartBuilderEditing = null;
  }

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

  // Layout panel states
  sidebarOpen = $state<boolean>(true);
  rightPanelOpen = $state<boolean>(true);
  sidebarWidth = $state<number>(256);
  rightPanelWidth = $state<number>(288);
  immersiveMode = $state<boolean>(false);
  isMiniplayer = $state<boolean>(false);
  // Guards enter/exitMiniplayerMode against overlapping calls: isMiniplayer
  // flips synchronously, but the window resize/decoration IPC round-trip it
  // guards is async, so a second toggle fired before that round-trip
  // resolves would read the window mid-transition and set a wrong "current
  // size" baseline — compounding into runaway growth on rapid toggling.
  private miniplayerTransitionInFlight = false;
  savedWindowWidth = $state<number>(1280);
  savedWindowHeight = $state<number>(800);
  savedWindowX = $state<number | null>(null);
  savedWindowY = $state<number | null>(null);
  miniplayerWidth = $state<number>(300);
  miniplayerHeight = $state<number>(360);
  miniplayerX = $state<number | null>(null);
  miniplayerY = $state<number | null>(null);


  watchFoldersRealtime = $state<boolean>(true);
  scanOnStartup = $state<boolean>(false);
  lastScanTime = $state<string | null>(null);

  constructor() {
    this.init();
  }

  private async init() {
    try {
      // Restore layout states from localStorage
      if (typeof window !== "undefined") {
        const savedSidebar = localStorage.getItem("layout_sidebarOpen");
        if (savedSidebar !== null) this.sidebarOpen = savedSidebar === "true";

        const savedRight = localStorage.getItem("layout_rightPanelOpen");
        if (savedRight !== null) this.rightPanelOpen = savedRight === "true";

        const savedSidebarWidth = localStorage.getItem("layout_sidebarWidth");
        if (savedSidebarWidth) this.sidebarWidth = parseInt(savedSidebarWidth, 10);

        const savedRightWidth = localStorage.getItem("layout_rightPanelWidth");
        if (savedRightWidth) this.rightPanelWidth = parseInt(savedRightWidth, 10);

        const savedImmersive = localStorage.getItem("layout_immersiveMode");
        if (savedImmersive !== null) this.immersiveMode = savedImmersive === "true";

        const savedMiniplayerWidth = localStorage.getItem("layout_miniplayerWidth");
        if (savedMiniplayerWidth) this.miniplayerWidth = parseInt(savedMiniplayerWidth, 10);

        const savedMiniplayerHeight = localStorage.getItem("layout_miniplayerHeight");
        if (savedMiniplayerHeight) this.miniplayerHeight = parseInt(savedMiniplayerHeight, 10);

        const savedSavedWidth = localStorage.getItem("layout_savedWindowWidth");
        if (savedSavedWidth) this.savedWindowWidth = parseInt(savedSavedWidth, 10);

        const savedSavedHeight = localStorage.getItem("layout_savedWindowHeight");
        if (savedSavedHeight) this.savedWindowHeight = parseInt(savedSavedHeight, 10);

        const savedWindowXStr = localStorage.getItem("layout_savedWindowX");
        if (savedWindowXStr !== null) this.savedWindowX = parseFloat(savedWindowXStr);

        const savedWindowYStr = localStorage.getItem("layout_savedWindowY");
        if (savedWindowYStr !== null) this.savedWindowY = parseFloat(savedWindowYStr);

        const savedMiniXStr = localStorage.getItem("layout_miniplayerX");
        if (savedMiniXStr !== null) this.miniplayerX = parseFloat(savedMiniXStr);

        const savedMiniYStr = localStorage.getItem("layout_miniplayerY");
        if (savedMiniYStr !== null) this.miniplayerY = parseFloat(savedMiniYStr);

        const savedIsMiniplayer = localStorage.getItem("layout_isMiniplayer");
        if (savedIsMiniplayer === "true") {
          setTimeout(() => {
            this.enterMiniplayerMode();
          }, 100);
        }

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

        const savedRecent = localStorage.getItem("luminous_recent_searches");
        if (savedRecent) {
          try {
            this.recentSearches = JSON.parse(savedRecent);
          } catch (e) {
            console.error("Failed to parse saved recentSearches:", e);
          }
        }
      }

      // Seed history with the restored (or default) view so Back/Forward
      // have a starting point instead of an empty stack on boot.
      this.recordHistory();

      await this.refreshDirectories();
      await this.refreshStats();
      await this.refreshLibrary();

      // Load scanning settings from backend settings
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings) {
        if (settings.watch_folders_realtime !== undefined) {
          this.watchFoldersRealtime = settings.watch_folders_realtime !== "false";
        }
        if (settings.scan_on_startup !== undefined) {
          this.scanOnStartup = settings.scan_on_startup === "true";
        }
        if (settings.last_scan_time) {
          this.lastScanTime = settings.last_scan_time;
        }

        // First-launch / new-version detection (#182) — fires a Milestone-tier
        // celebration toast when the app is launched for the first time or
        // after a version upgrade. Stores the version string so the welcome
        // replays on each new release, not just the very first launch.
        let appVersion = "";
        try {
          appVersion = await invoke<string>("get_app_version");
        } catch {
          try {
            const { getVersion } = await import("@tauri-apps/api/app");
            appVersion = await getVersion();
          } catch { /* not in Tauri context */ }
        }
        if (appVersion && settings.launched_version !== appVersion) {
          this.isFirstLaunch = true;
          invoke("set_app_setting", { key: "launched_version", value: appVersion }).catch((err) => {
            console.error("Failed to persist launched_version:", err);
          });
          // Show the welcome toast after a short delay so the UI has time to
          // render before the celebration.
          const isFirstEver = !settings.launched_version;
          setTimeout(() => {
            const msg = isFirstEver
              ? i18n.t("celebrations.firstLaunch", { version: appVersion }, `Welcome to Luminous v${appVersion}!`)
              : i18n.t("celebrations.newVersion", { version: appVersion }, `Updated to Luminous v${appVersion}`);
            toastStore.show(msg, "milestone");
            setTimeout(() => { this.isFirstLaunch = false; }, 700);
          }, 1200);
        }
      }

      // Listen to library scan progress events
      await listen<ScanProgress>("scan-progress", (event) => {
        this.scanProgress = event.payload;
        this.isScanning = event.payload.phase !== "done";
        if (event.payload.phase === "done") {
          const nowStr = new Date().toLocaleString();
          this.lastScanTime = nowStr;
          invoke("set_app_setting", { key: "last_scan_time", value: nowStr }).catch((err) => {
            console.error("Failed to save last_scan_time:", err);
          });
          this.refreshDirectories();
          const songCountBeforeRefresh = this.stats.total_songs;
          this.refreshStats().then(() => {
            const added = this.stats.total_songs - songCountBeforeRefresh;
            if (added > 0) {
              toastStore.show(i18n.t("settings.importFinishedToast", { count: added }), "success");
            }

            // Milestone detection (#182): check if total_songs just crossed a
            // threshold. Only fire the first one crossed (don't stack multiple
            // milestone toasts if an import jumps past several at once).
            const newTotal = this.stats.total_songs;
            for (const threshold of MILESTONE_THRESHOLDS) {
              if (songCountBeforeRefresh < threshold && newTotal >= threshold) {
                this.milestoneReached = threshold;
                toastStore.show(
                  i18n.t("celebrations.milestone", { count: threshold.toLocaleString() }, `${threshold.toLocaleString()} songs in your library!`),
                  "milestone"
                );
                setTimeout(() => { this.milestoneReached = null; }, 700);
                break;
              }
            }
          });
          this.refreshLibrary();

          // The active playback queue is a snapshot taken when it was built —
          // a scan that repoints a moved file's path (or drops a genuinely
          // missing one) fixes the database but not an already-queued track,
          // so resync it here rather than requiring a restart.
          invoke("refresh_playback_queue").catch((err) => {
            console.error("Failed to refresh playback queue after scan:", err);
          });

          // Genre/decade auto-playlists otherwise only regenerate on their own
          // 24h staleness window (or whenever the Playlists tab happens to
          // mount) — a scan can add new genres/decades or shift which songs
          // qualify, so rebuild them right away instead of leaving them stale.
          (async () => {
            try {
              await invoke("sync_genre_auto_playlists");
              await invoke("sync_decade_auto_playlists");
              await playlistsStore.refreshPlaylists();
            } catch (err) {
              console.error("Failed to sync auto-playlists after scan:", err);
            }
          })();
        }
      });

      // Listen to library changed events (e.g. from background directory watcher,
      // or a song getting flagged unavailable after a failed play)
      await listen("library-changed", () => {
        this.refreshStats();
        this.refreshLibrary();
        this.refreshDirectories();
        invoke("refresh_playback_queue").catch((err) => {
          console.error("Failed to refresh playback queue after library change:", err);
        });
      });

      // Keep cached song rows in sync with rating/playcount changes made
      // anywhere in the app (player bar, other views, scrobble bumps).
      await listen<SongStatsPayload>("song-stats-changed", (event) => {
        for (const list of [this.songs, this.searchResults]) {
          const song = list.find((s) => s.id === event.payload.song_id);
          if (song) applySongStats(song, event.payload);
        }
      });

      if (this.scanOnStartup) {
        this.startScan(false);
      }
    } catch (err) {
      console.error("Failed to initialize CollectionStore:", err);
    }
  }

  async setWatchFoldersRealtime(enabled: boolean) {
    this.watchFoldersRealtime = enabled;
    await invoke("set_app_setting", { key: "watch_folders_realtime", value: String(enabled) }).catch(err => {
      console.error("Failed to save watch_folders_realtime setting:", err);
    });
  }

  async setScanOnStartup(enabled: boolean) {
    this.scanOnStartup = enabled;
    await invoke("set_app_setting", { key: "scan_on_startup", value: String(enabled) }).catch(err => {
      console.error("Failed to save scan_on_startup setting:", err);
    });
  }

  async refreshDirectories() {
    this.directories = await invoke("get_directories");
  }

  /**
   * True when `path` lives under a watched directory that's currently
   * unreachable (disconnected USB drive, sleeping network share, etc).
   * Distinct from `song.unavailable`, which only flips once the backend has
   * confirmed the file is actually gone — a song on a merely-disconnected
   * drive still reads as "available" there (see collection.rs's
   * find_missing_song_ids doc comment) until a play is attempted or the
   * user un-watches the folder. This lets the UI show a "disconnected"
   * state proactively instead of waiting for a failed play.
   */
  isPathOnDisconnectedDrive(path: string | null | undefined): boolean {
    if (!path) return false;
    return this.directories.some((d) => d.is_available === false && path.startsWith(d.path));
  }

  async refreshStats() {
    this.stats = await invoke("get_library_stats");
    this.statsLoaded = true;
  }

  async refreshLibrary() {
    this.songs = await invoke("get_songs", { limit: 1000, offset: 0 });
    this.albums = await invoke("get_albums");
    this.artists = await invoke("get_artists");
  }

  async addDirectory(path: string) {
    const wasEmpty = this.directories.length === 0;
    await invoke("add_directory", { path });
    await this.refreshDirectories();

    // First watched folder celebration (#182, New tier).
    if (wasEmpty && this.directories.length > 0) {
      this.justAddedFirstFolder = true;
      toastStore.show(i18n.t("celebrations.firstFolder", {}, "First music folder added!"), "success");
      setTimeout(() => { this.justAddedFirstFolder = false; }, 400);
    }

    this.startScan(false);
  }

  async addDirectoryDialog() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: i18n.t('settings.selectMusicDirectory'),
      });
      if (selected && typeof selected === "string") {
        await this.addDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open directory dialog:", err);
    }
  }

  async removeDirectory(path: string) {
    await invoke("remove_directory", { path });
    await this.refreshDirectories();
    this.startScan(false);
  }

  async startScan(force: boolean = false) {
    this.isScanning = true;
    invoke("scan_directories", { force }).catch((err) => {
      console.error("Failed to scan directories:", err);
      this.isScanning = false;
    });
  }

  async pruneMissing(): Promise<{ deletedSongs: number; removedFolders: number }> {
    try {
      const result = await invoke<{ deleted_songs: number; removed_folders: number }>("prune_missing_songs");
      await this.refreshStats();
      await this.refreshLibrary();
      return { deletedSongs: result.deleted_songs, removedFolders: result.removed_folders };
    } catch (err) {
      console.error("Failed to prune missing songs:", err);
      return { deletedSongs: 0, removedFolders: 0 };
    }
  }

  async search(query: string) {
    if (query.trim() !== "") {
      this.selectedArtistName = null;
      this.selectedAlbumName = null;
    }
    this.searchQuery = query;
    if (query.trim() === "") {
      this.searchResults = [];
      return;
    }
    this.searchLoading = true;
    try {
      this.searchResults = await invoke("search_songs", { query, limit: 500 });
    } catch (err) {
      console.error("Failed to execute search:", err);
    } finally {
      this.searchLoading = false;
    }
  }

  saveRecentSearches() {
    if (typeof window !== "undefined") {
      localStorage.setItem("luminous_recent_searches", JSON.stringify(this.recentSearches));
    }
  }

  addRecentSearch(item: Omit<RecentSearchItem, "id" | "timestamp">) {
    const cleanTitle = (item.title || "").trim();
    if (!cleanTitle) return;

    // Deduplicate by title + kind
    const existingIndex = this.recentSearches.findIndex(
      (r) => r.kind === item.kind && r.title.toLowerCase() === cleanTitle.toLowerCase()
    );
    if (existingIndex !== -1) {
      this.recentSearches.splice(existingIndex, 1);
    }

    const newItem: RecentSearchItem = {
      ...item,
      id: `rs_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`,
      title: cleanTitle,
      timestamp: Date.now(),
    };

    this.recentSearches.unshift(newItem);
    if (this.recentSearches.length > MAX_RECENT_SEARCHES) {
      this.recentSearches = this.recentSearches.slice(0, MAX_RECENT_SEARCHES);
    }
    this.saveRecentSearches();
  }

  removeRecentSearch(id: string) {
    this.recentSearches = this.recentSearches.filter((r) => r.id !== id);
    this.saveRecentSearches();
  }

  clearRecentSearches() {
    this.recentSearches = [];
    this.saveRecentSearches();
  }

  viewArtist(name: string) {
    this.searchQuery = "";
    this.searchResults = [];
    this.selectedAlbumName = null;
    this.activeTab = "collection";
    this.activeSubTab = "artists";
    this.selectedArtistName = name;
  }

  viewAlbum(name: string) {
    this.searchQuery = "";
    this.searchResults = [];
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

  toggleSidebar() {
    this.sidebarOpen = !this.sidebarOpen;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_sidebarOpen", this.sidebarOpen.toString());
    }
  }

  toggleImmersiveMode() {
    this.immersiveMode = !this.immersiveMode;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_immersiveMode", this.immersiveMode.toString());
    }
  }

  // Force immersive mode off — used when there's nothing playing, since the
  // only way back out is a toggle on the PlayerBar, which is itself hidden
  // until a track has ever played this session (issue #71).
  exitImmersiveMode() {
    if (!this.immersiveMode) return;
    this.immersiveMode = false;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_immersiveMode", "false");
    }
  }

  setSavedWindowGeometry(width: number, height: number, x?: number | null, y?: number | null) {
    this.savedWindowWidth = Math.max(900, Math.round(width));
    this.savedWindowHeight = Math.max(600, Math.round(height));
    if (x !== undefined && x !== null) this.savedWindowX = Math.round(x);
    if (y !== undefined && y !== null) this.savedWindowY = Math.round(y);
    console.log(`[CollectionStore] Saved Full Player Geometry: ${this.savedWindowWidth}x${this.savedWindowHeight} at (${this.savedWindowX}, ${this.savedWindowY})`);
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_savedWindowWidth", this.savedWindowWidth.toString());
      localStorage.setItem("layout_savedWindowHeight", this.savedWindowHeight.toString());
      if (this.savedWindowX !== null) localStorage.setItem("layout_savedWindowX", this.savedWindowX.toString());
      if (this.savedWindowY !== null) localStorage.setItem("layout_savedWindowY", this.savedWindowY.toString());
    }
  }

  setMiniplayerGeometry(width: number, height: number, x?: number | null, y?: number | null) {
    this.miniplayerWidth = Math.max(300, Math.round(width));
    this.miniplayerHeight = Math.max(360, Math.round(height));
    if (x !== undefined && x !== null) this.miniplayerX = Math.round(x);
    if (y !== undefined && y !== null) this.miniplayerY = Math.round(y);
    console.log(`[CollectionStore] Saved Miniplayer Geometry: ${this.miniplayerWidth}x${this.miniplayerHeight} at (${this.miniplayerX}, ${this.miniplayerY})`);
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_miniplayerWidth", this.miniplayerWidth.toString());
      localStorage.setItem("layout_miniplayerHeight", this.miniplayerHeight.toString());
      if (this.miniplayerX !== null) localStorage.setItem("layout_miniplayerX", this.miniplayerX.toString());
      if (this.miniplayerY !== null) localStorage.setItem("layout_miniplayerY", this.miniplayerY.toString());
    }
  }

  setMiniplayerSize(width: number, height: number) {
    this.setMiniplayerGeometry(width, height);
  }

  async enterMiniplayerMode(
    width = this.miniplayerWidth,
    height = this.miniplayerHeight,
    x = this.miniplayerX,
    y = this.miniplayerY
  ) {
    if (this.isMiniplayer || this.miniplayerTransitionInFlight) return;
    this.miniplayerTransitionInFlight = true;
    this.isMiniplayer = true;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_isMiniplayer", "true");
    }
    console.log(`[CollectionStore] Entering Miniplayer Mode target: ${width}x${height} at (${x}, ${y})`);
    try {
      const payload: {
        width: number;
        height: number;
        x?: number;
        y?: number;
        savedFullWidth?: number;
        savedFullHeight?: number;
      } = { width, height };
      if (x !== null && x !== undefined) payload.x = x;
      if (y !== null && y !== undefined) payload.y = y;
      if (this.savedWindowWidth > 0) payload.savedFullWidth = this.savedWindowWidth;
      if (this.savedWindowHeight > 0) payload.savedFullHeight = this.savedWindowHeight;

      const res = await invoke<{
        saved_width: number;
        saved_height: number;
        saved_x?: number | null;
        saved_y?: number | null;
      }>("enter_miniplayer_mode", payload);

      console.log(`[CollectionStore] enter_miniplayer_mode returned:`, res);
      if (res && res.saved_width && res.saved_height) {
        this.setSavedWindowGeometry(
          res.saved_width,
          res.saved_height,
          res.saved_x,
          res.saved_y
        );
      }
    } catch (e) {
      console.warn("Failed to enter miniplayer backend window mode:", e);
    } finally {
      setTimeout(() => {
        this.miniplayerTransitionInFlight = false;
      }, 500);
    }
  }

  async exitMiniplayerMode() {
    if (!this.isMiniplayer || this.miniplayerTransitionInFlight) return;
    this.miniplayerTransitionInFlight = true;
    this.isMiniplayer = false;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_isMiniplayer", "false");
    }
    console.log(`[CollectionStore] Exiting Miniplayer Mode, restoring Full Player: ${this.savedWindowWidth}x${this.savedWindowHeight} at (${this.savedWindowX}, ${this.savedWindowY})`);
    try {
      const payload: {
        savedWidth: number;
        savedHeight: number;
        savedX?: number;
        savedY?: number;
      } = {
        savedWidth: this.savedWindowWidth,
        savedHeight: this.savedWindowHeight,
      };
      if (this.savedWindowX !== null) payload.savedX = this.savedWindowX;
      if (this.savedWindowY !== null) payload.savedY = this.savedWindowY;

      const res = await invoke<{
        mini_width: number;
        mini_height: number;
        mini_x?: number | null;
        mini_y?: number | null;
      }>("exit_miniplayer_mode", payload);

      console.log(`[CollectionStore] exit_miniplayer_mode returned:`, res);
      if (res && res.mini_width && res.mini_height) {
        this.setMiniplayerGeometry(
          res.mini_width,
          res.mini_height,
          res.mini_x,
          res.mini_y
        );
      }
    } catch (e) {
      console.warn("Failed to exit miniplayer backend window mode:", e);
    } finally {
      setTimeout(() => {
        this.miniplayerTransitionInFlight = false;
      }, 500);
    }
  }

  async moveWindowToPreset(position: string) {
    try {
      await invoke("move_window_to_preset", { position });
    } catch (e) {
      console.warn("Failed to move window to preset:", e);
    }
  }

  async toggleMiniplayerMode() {
    if (this.isMiniplayer) {
      await this.exitMiniplayerMode();
    } else {
      await this.enterMiniplayerMode();
    }
  }


  toggleRightPanel() {
    this.rightPanelOpen = !this.rightPanelOpen;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_rightPanelOpen", this.rightPanelOpen.toString());
    }
  }

  setSidebarWidth(width: number) {
    this.sidebarWidth = width;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_sidebarWidth", width.toString());
    }
  }

  setRightPanelWidth(width: number) {
    this.rightPanelWidth = width;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_rightPanelWidth", width.toString());
    }
  }

  get filteredSongs(): Song[] {
    return this.searchQuery.trim() === "" ? this.songs : this.searchResults;
  }

  get filteredAlbums(): AlbumItem[] {
    const query = this.searchQuery.trim().toLowerCase();
    if (query === "") return this.albums;
    return this.albums.filter(album => 
      (album.album && album.album.toLowerCase().includes(query)) ||
      (album.artist && album.artist.toLowerCase().includes(query))
    );
  }

  get filteredArtists(): ArtistItem[] {
    const query = this.searchQuery.trim().toLowerCase();
    if (query === "") return this.artists;
    return this.artists.filter(artist => 
      artist.name && artist.name.toLowerCase().includes(query)
    );
  }
}

export const collectionStore = new CollectionStore();
