import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Song, MusicDirectory, LibraryStats, ScanProgress, AlbumItem, ArtistItem } from "../types";
import { applySongStats, type SongStatsPayload } from "../utils/stats";

class CollectionStore {
  directories = $state<MusicDirectory[]>([]);
  stats = $state<LibraryStats>({
    total_songs: 0,
    total_artists: 0,
    total_albums: 0,
    total_duration_nanosec: 0,
    total_filesize_bytes: 0,
  });
  isScanning = $state<boolean>(false);
  scanProgress = $state<ScanProgress | null>(null);
  excludedFormats = $state<string[]>([]);

  // Cached collections
  songs = $state<Song[]>([]);
  albums = $state<AlbumItem[]>([]);
  artists = $state<ArtistItem[]>([]);
  searchResults = $state<Song[]>([]);
  searchQuery = $state<string>("");
  searchLoading = $state<boolean>(false);
  private _activeTab = $state<"home" | "collection" | "playlists" | "settings" | "lyrics">("collection");
  private _activeSubTab = $state<"songs" | "albums" | "artists">("songs");

  get activeTab() { return this._activeTab; }
  set activeTab(val) {
    this._activeTab = val;
    if (typeof window !== "undefined") {
      localStorage.setItem("navigation_activeTab", val);
    }
  }

  get activeSubTab() { return this._activeSubTab; }
  set activeSubTab(val) {
    this._activeSubTab = val;
    if (typeof window !== "undefined") {
      localStorage.setItem("navigation_activeSubTab", val);
    }
  }

  // Layout panel states
  sidebarOpen = $state<boolean>(true);
  rightPanelOpen = $state<boolean>(true);
  sidebarWidth = $state<number>(256);
  rightPanelWidth = $state<number>(288);
  immersiveMode = $state<boolean>(false);

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

        const savedTab = localStorage.getItem("navigation_activeTab");
        if (savedTab) this._activeTab = savedTab as any;

        const savedSubTab = localStorage.getItem("navigation_activeSubTab");
        if (savedSubTab) this._activeSubTab = savedSubTab as any;
      }

      await this.refreshDirectories();
      await this.refreshStats();
      await this.refreshLibrary();

      // Load excluded formats from backend settings
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.excluded_formats) {
        try {
          this.excludedFormats = JSON.parse(settings.excluded_formats);
        } catch (e) {
          console.error("Failed to parse excluded_formats:", e);
        }
      }

      // Listen to library scan progress events
      await listen<ScanProgress>("scan-progress", (event) => {
        this.scanProgress = event.payload;
        this.isScanning = event.payload.phase !== "done";
        if (event.payload.phase === "done") {
          this.refreshStats();
          this.refreshLibrary();
        }
      });

      // Listen to library changed events (e.g. from background directory watcher)
      await listen("library-changed", () => {
        this.refreshStats();
        this.refreshLibrary();
      });

      // Keep cached song rows in sync with rating/playcount changes made
      // anywhere in the app (player bar, other views, scrobble bumps).
      await listen<SongStatsPayload>("song-stats-changed", (event) => {
        for (const list of [this.songs, this.searchResults]) {
          const song = list.find((s) => s.id === event.payload.song_id);
          if (song) applySongStats(song, event.payload);
        }
      });
    } catch (err) {
      console.error("Failed to initialize CollectionStore:", err);
    }
  }

  async refreshDirectories() {
    this.directories = await invoke("get_directories");
  }

  async refreshStats() {
    this.stats = await invoke("get_library_stats");
  }

  async refreshLibrary() {
    this.songs = await invoke("get_songs", { limit: 1000, offset: 0 });
    this.albums = await invoke("get_albums");
    this.artists = await invoke("get_artists");
  }

  async addDirectory(path: string) {
    await invoke("add_directory", { path });
    await this.refreshDirectories();
  }

  async removeDirectory(path: string) {
    await invoke("remove_directory", { path });
    await this.refreshDirectories();
  }

  async startScan() {
    this.isScanning = true;
    invoke("scan_directories").catch((err) => {
      console.error("Failed to scan directories:", err);
      this.isScanning = false;
    });
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

  navigateTo(tab: "home" | "collection" | "playlists" | "settings" | "lyrics", subTab?: "songs" | "albums" | "artists", query?: string) {
    this.selectedArtistName = null;
    this.selectedAlbumName = null;
    this.activeTab = tab;
    if (subTab) {
      this.activeSubTab = subTab;
    }
    if (query !== undefined) {
      this.searchQuery = query;
    }
  }

  /** Selected artist for the Artist Detail view (rendered inside CollectionView). */
  selectedArtistName = $state<string | null>(null);

  /** Selected album for the Album Detail view (rendered inside CollectionView). */
  selectedAlbumName = $state<string | null>(null);

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

  isFormatExcluded(filetype: string): boolean {
    const ft = (filetype || "").toUpperCase();
    for (const excluded of this.excludedFormats) {
      if (excluded === "OGG" && ft.startsWith("OGG_")) {
        return true;
      }
      if (ft === excluded) {
        return true;
      }
    }
    return false;
  }

  async toggleFormat(format: string) {
    if (this.excludedFormats.includes(format)) {
      this.excludedFormats = this.excludedFormats.filter(f => f !== format);
    } else {
      this.excludedFormats.push(format);
    }
    await invoke("set_app_setting", { key: "excluded_formats", value: JSON.stringify(this.excludedFormats) }).catch(err => {
      console.error("Failed to save excluded_formats:", err);
    });
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
    let result = this.searchQuery.trim() === "" ? this.songs : this.searchResults;
    return result.filter(song => !this.isFormatExcluded(song.filetype));
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
