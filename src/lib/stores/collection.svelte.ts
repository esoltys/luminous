import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { i18n } from "./i18n.svelte";
import type {
  Song,
  MusicDirectory,
  LibraryStats,
  DbSchemaStatus,
  ScanProgress,
  BatchProgress,
  AlbumItem,
  ArtistItem,
  ArtistProfile,
  RecentSearchItem,
  QueuePopulationMode,
} from "../types";
import { applySongStats, type SongStatsPayload, applyAlbumStats, type AlbumStatsPayload } from "../utils/stats";
import { navigationStore } from "./navigation.svelte";
import { playlistsStore } from "./playlists.svelte";
import { tagsStore } from "./tags.svelte";
import { toastStore } from "./toast.svelte";
import { MAX_RECENT_SEARCHES } from "../constants";

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

/**
 * Saved pixel widths for song-table columns, keyed by `VisibleColumns` property name.
 * Only columns that have been explicitly resized have an entry; others fall back to
 * their compile-time defaults.  A single shared map is used for all four table views
 * (Collection, AlbumDetail, Playlist, AutoPlaylist).
 */
export type ColumnWidths = Partial<Record<keyof VisibleColumns, number>>;

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
  /** Set once at startup. When `db_newer_than_app` is true, the library looking
   *  empty isn't real emptiness — this build can't read data written by whatever
   *  newer build last touched the database. LibraryWelcome swaps its message
   *  based on this instead of the normal "add a folder" copy. */
  dbSchemaStatus = $state<DbSchemaStatus | null>(null);
  isScanning = $state<boolean>(false);
  scanProgress = $state<ScanProgress | null>(null);

  /** Tracks the single toast representing the currently in-flight file-watcher
   *  batch (#233), so its progress collapses into one notification instead of
   *  a toast per file. */
  private activeBatchToast: { batchId: number; toastId: number } | null = null;

  /** Celebration moment states (issue #182) — consumed by Toast and layout. */
  isFirstLaunch = $state<boolean>(false);
  justAddedFirstFolder = $state<boolean>(false);
  milestoneReached = $state<number | null>(null);

  // Cached collections
  songs = $state<Song[]>([]);
  albums = $state<AlbumItem[]>([]);
  artists = $state<ArtistItem[]>([]);
  artistProfiles = $state<Record<string, ArtistProfile>>({});
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

  columnWidths = $state<ColumnWidths>(
    (() => {
      if (typeof window !== "undefined") {
        const saved = localStorage.getItem("luminous_column_widths");
        if (saved) {
          try {
            return JSON.parse(saved) as ColumnWidths;
          } catch (e) {
            console.error("Failed to parse column widths:", e);
          }
        }
      }
      return {};
    })()
  );

  setColumnWidth(column: keyof VisibleColumns, widthPx: number) {
    this.columnWidths[column] = widthPx;
    if (typeof window !== "undefined") {
      localStorage.setItem("luminous_column_widths", JSON.stringify(this.columnWidths));
    }
  }

  resetColumnWidth(column: keyof VisibleColumns) {
    delete this.columnWidths[column];
    if (typeof window !== "undefined") {
      localStorage.setItem("luminous_column_widths", JSON.stringify(this.columnWidths));
    }
  }

  isSmartBuilderOpen = $state<boolean>(false);
  smartBuilderRules = $state<Array<{ field: string; op: string; value: string }>>([]);
  smartBuilderEditing = $state<{
    id: number;
    name: string;
    populationMode?: QueuePopulationMode;
  } | null>(null);

  openSmartBuilder(
    rules?: Array<{ field: string; op: string; value: string }>,
    editing?: { id: number; name: string; populationMode?: QueuePopulationMode }
  ) {
    this.smartBuilderRules = rules || [];
    this.smartBuilderEditing = editing ?? null;
    this.isSmartBuilderOpen = true;
    navigationStore.activeTab = "playlists";
    navigationStore.playlistsSubTab = "custom";
  }

  closeSmartBuilder() {
    this.isSmartBuilderOpen = false;
    this.smartBuilderRules = [];
    this.smartBuilderEditing = null;
  }

  watchFoldersRealtime = $state<boolean>(true);
  scanOnStartup = $state<boolean>(false);
  lastScanTime = $state<string | null>(null);

  constructor() {
    this.init();
  }

  private async init() {
    try {
      if (typeof window !== "undefined") {
        const savedRecent = localStorage.getItem("luminous_recent_searches");
        if (savedRecent) {
          try {
            this.recentSearches = JSON.parse(savedRecent);
          } catch (e) {
            console.error("Failed to parse saved recentSearches:", e);
          }
        }
      }

      await this.refreshDirectories();
      await this.refreshDbSchemaStatus();
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
          invoke("set_app_setting", { key: "launched_version", value: appVersion });
          // Show the welcome toast after a short delay so the UI has time to
          // render before the celebration.
          const isFirstEver = !settings.launched_version;
          setTimeout(() => {
            const msg = isFirstEver
              ? i18n.t("celebrations.firstLaunch", { version: appVersion }, `Welcome to Luminous v${appVersion}!`)
              : i18n.t("celebrations.newVersion", { version: appVersion }, `Updated to Luminous v${appVersion}`);
            const releaseUrl = isFirstEver
              ? undefined
              : "https://github.com/esoltys/luminous/releases";
            toastStore.show(msg, "milestone", undefined, releaseUrl);
            setTimeout(() => { this.isFirstLaunch = false; }, 700);
          }, 1200);
        }
      }

      await listen<ScanProgress>("scan-progress", (event) => {
        this.scanProgress = event.payload;
        this.isScanning = event.payload.phase !== "done";
        if (event.payload.phase === "done") {
          const nowStr = new Date().toLocaleString();
          this.lastScanTime = nowStr;
          this.refreshDirectories();
          const songCountBeforeRefresh = this.stats.total_songs;
          this.refreshStats().then(() => {
            const added = this.stats.total_songs - songCountBeforeRefresh;
            // A `silent` scan is a watcher-triggered catch-up rescan (overflow
            // recovery, or a newly-appeared directory), not an explicit user
            // action — the watcher's own batch-processing toast already
            // covers this same filesystem activity, so skip this toast to
            // avoid a second, less-accurate "songs added" notification (#233).
            if (added > 0 && !event.payload.silent) {
              const text = added === 1
                ? i18n.t("settings.importFinishedToastOne")
                : i18n.t("settings.importFinishedToastMany", { count: added });
              toastStore.show(text, "success");
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

          // The backend persists last_scan_time, resyncs the live playback
          // queue (a scan can repoint a moved file's path or drop a
          // genuinely missing one out from under an already-queued track),
          // and rebuilds genre/decade/BPM auto-playlists (which otherwise
          // only regenerate on their own 24h staleness window) in one call.
          (async () => {
            try {
              await invoke("finish_scan", { lastScanTime: nowStr });
              await playlistsStore.refreshPlaylists();
            } catch (err) {
              console.error("Failed to finish scan sync:", err);
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
        tagsStore.load().catch((err) => {
          console.error("Failed to refresh tags after library change:", err);
        });
        invoke("refresh_playback_queue").catch((err) => {
          console.error("Failed to refresh playback queue after library change:", err);
        });
      });

      // Collapse a whole debounced file-watcher batch (#233) into one toast
      // that updates in place, instead of one toast per file it touches.
      await listen<BatchProgress>("batch-processing-started", (event) => {
        const { batch_id, total_count } = event.payload;
        const toastId = toastStore.startBatch(
          i18n.t("settings.batchProcessingToast", { current: 0, total: total_count })
        );
        this.activeBatchToast = { batchId: batch_id, toastId };
      });

      await listen<BatchProgress>("batch-processing-progress", (event) => {
        const { batch_id, current_count, total_count } = event.payload;
        if (this.activeBatchToast?.batchId !== batch_id) return;
        toastStore.updateBatch(
          this.activeBatchToast.toastId,
          i18n.t("settings.batchProcessingToast", { current: current_count, total: total_count })
        );
      });

      await listen<BatchProgress>("batch-processing-completed", (event) => {
        const { batch_id, total_count } = event.payload;
        if (this.activeBatchToast?.batchId !== batch_id) return;
        const text = total_count === 1
          ? i18n.t("settings.batchProcessingDoneToastOne")
          : i18n.t("settings.batchProcessingDoneToastMany", { count: total_count });
        toastStore.finishBatch(
          this.activeBatchToast.toastId,
          text,
          "success"
        );
        this.activeBatchToast = null;
      });

      // Keep cached song rows in sync with rating/playcount changes made
      // anywhere in the app (player bar, other views, scrobble bumps).
      await listen<SongStatsPayload>("song-stats-changed", (event) => {
        for (const list of [this.songs, this.searchResults]) {
          const song = list.find((s) => s.id === event.payload.song_id);
          if (song) applySongStats(song, event.payload);
        }
      });

      // Keep cached album rows in sync with ratings set from other views
      // (Album Detail view, other Collection grid instances).
      await listen<AlbumStatsPayload>("album-stats-changed", (event) => {
        const album = this.albums.find((a) => a.album === event.payload.album);
        if (album) applyAlbumStats(album, event.payload);
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
    await invoke("set_app_setting", { key: "watch_folders_realtime", value: String(enabled) });
  }

  async setScanOnStartup(enabled: boolean) {
    this.scanOnStartup = enabled;
    await invoke("set_app_setting", { key: "scan_on_startup", value: String(enabled) });
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

  async refreshDbSchemaStatus() {
    try {
      this.dbSchemaStatus = await invoke<DbSchemaStatus>("get_db_schema_status");
    } catch (err) {
      console.error("Failed to load db schema status:", err);
    }
  }

  async refreshLibrary() {
    const snapshot = await invoke<{ songs: Song[]; albums: AlbumItem[]; artists: ArtistItem[] }>(
      "get_library_snapshot"
    );
    this.songs = snapshot.songs;
    this.albums = snapshot.albums;
    this.artists = snapshot.artists;
    await this.loadArtistProfiles();
  }

  async loadArtistProfiles() {
    try {
      const profiles = await invoke<ArtistProfile[]>("get_all_artist_profiles");
      const map: Record<string, ArtistProfile> = {};
      for (const p of profiles) {
        if (p.artist_key) {
          map[p.artist_key.toLowerCase()] = p;
        }
      }
      this.artistProfiles = map;
    } catch (err) {
      console.error("Failed to load artist profiles:", err);
    }
  }

  getArtistProfile(artistName: string | null | undefined): ArtistProfile | undefined {
    if (!artistName) return undefined;
    return this.artistProfiles[artistName.toLowerCase()];
  }

  async saveArtistProfile(profile: ArtistProfile): Promise<ArtistProfile> {
    const saved = await invoke<ArtistProfile>("set_artist_profile", { profile });
    if (saved?.artist_key) {
      this.artistProfiles = {
        ...this.artistProfiles,
        [saved.artist_key.toLowerCase()]: saved,
      };
    }
    return saved;
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

  async pruneMissing(): Promise<{ deletedSongs: number; removedFolders: number; mergedDuplicates: number }> {
    try {
      const result = await invoke<{ deleted_songs: number; removed_folders: number; merged_duplicates: number }>("prune_missing_songs");
      await this.refreshStats();
      await this.refreshLibrary();
      return {
        deletedSongs: result.deleted_songs,
        removedFolders: result.removed_folders,
        mergedDuplicates: result.merged_duplicates,
      };
    } catch (err) {
      console.error("Failed to prune missing songs:", err);
      return { deletedSongs: 0, removedFolders: 0, mergedDuplicates: 0 };
    }
  }

  async search(query: string) {
    if (query.trim() !== "") {
      navigationStore.selectedArtistName = null;
      navigationStore.selectedAlbumName = null;
    }
    this.searchQuery = query;
    if (query.trim() === "") {
      this.searchResults = [];
      return;
    }
    this.searchLoading = true;
    try {
      const results = await invoke<Song[]>("search_songs", { query, limit: 500 });
      this.searchResults = Array.isArray(results) ? results : [];
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

  get filteredSongs(): Song[] {
    return this.searchQuery.trim() === "" ? this.songs : this.searchResults;
  }

  get filteredAlbums(): AlbumItem[] {
    const rawQuery = this.searchQuery.trim();
    if (rawQuery === "") return this.albums;

    // Check if query is structured search with artist-tag or tag
    const tagMatch = rawQuery.match(/^(?:artist[-_]?tags?|artisttags?|tags?):(.+)$/i);
    if (tagMatch) {
      const tagQuery = tagMatch[1].replace(/^['"]|['"]$/g, "").trim().toLowerCase();
      if (!tagQuery) return this.albums;
      return this.albums.filter((album) => {
        if (!album.artist) return false;
        const profile = this.artistProfiles[album.artist.toLowerCase()];
        return profile?.tags?.some((t) => t.toLowerCase().includes(tagQuery)) ?? false;
      });
    }

    const query = rawQuery.toLowerCase();
    return this.albums.filter((album) => {
      if (album.album && album.album.toLowerCase().includes(query)) return true;
      if (album.artist && album.artist.toLowerCase().includes(query)) return true;
      if (album.artist) {
        const profile = this.artistProfiles[album.artist.toLowerCase()];
        if (profile?.tags?.some((t) => t.toLowerCase().includes(query))) return true;
      }
      return false;
    });
  }

  get filteredArtists(): ArtistItem[] {
    const rawQuery = this.searchQuery.trim();
    if (rawQuery === "") return this.artists;

    // Check if query is structured search with artist-tag or tag
    const tagMatch = rawQuery.match(/^(?:artist[-_]?tags?|artisttags?|tags?):(.+)$/i);
    if (tagMatch) {
      const tagQuery = tagMatch[1].replace(/^['"]|['"]$/g, "").trim().toLowerCase();
      if (!tagQuery) return this.artists;
      return this.artists.filter((artist) => {
        if (!artist.name) return false;
        const profile = this.artistProfiles[artist.name.toLowerCase()];
        return profile?.tags?.some((t) => t.toLowerCase().includes(tagQuery)) ?? false;
      });
    }

    const query = rawQuery.toLowerCase();
    return this.artists.filter((artist) => {
      if (!artist.name) return false;
      if (artist.name.toLowerCase().includes(query)) return true;
      const profile = this.artistProfiles[artist.name.toLowerCase()];
      return profile?.tags?.some((t) => t.toLowerCase().includes(query)) ?? false;
    });
  }
}

export const collectionStore = new CollectionStore();
