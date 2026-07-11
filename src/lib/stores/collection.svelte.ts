import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Song, MusicDirectory, LibraryStats, ScanProgress, AlbumItem, ArtistItem } from "../types";

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
  activeTab = $state<"collection" | "playlists" | "settings" | "lyrics">("collection");
  activeSubTab = $state<"songs" | "albums" | "artists">("songs");

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

  navigateTo(tab: "collection" | "playlists" | "settings" | "lyrics", subTab?: "songs" | "albums" | "artists", query?: string) {
    this.activeTab = tab;
    if (subTab) {
      this.activeSubTab = subTab;
    }
    if (query !== undefined) {
      this.searchQuery = query;
    }
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
}

export const collectionStore = new CollectionStore();
