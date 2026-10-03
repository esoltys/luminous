import { invoke } from "@tauri-apps/api/core";

export type RatingStyle = "heart" | "stars" | "both";
type SeekBarMode = "waveform" | "bands";
export type CollectionViewMode = "cards" | "rows";
export type GenreSortField = "name" | "count";
export type WeekStart = "sunday" | "monday";

/** Shape of the backend's UiPreferences struct — the schema (keys, domains,
 * defaults) lives in Rust (commands/settings.rs); this store just mirrors it. */
interface UiPreferences {
  rating_style: RatingStyle;
  seekbar_mode: SeekBarMode;
  fanart_api_key: string;
  fanart_fetch_photo: boolean;
  fanart_fetch_logo: boolean;
  fanart_fetch_background: boolean;
  fanart_fetch_album_cover: boolean;
  fanart_fetch_disc_art: boolean;
  save_artwork_to_folders: boolean;
  albums_view_mode: CollectionViewMode;
  artists_view_mode: CollectionViewMode;
  playlists_auto_view_mode: CollectionViewMode;
  playlists_custom_view_mode: CollectionViewMode;
  genre_view_mode: string;
  genre_cards_view_mode: CollectionViewMode;
  genre_sort_field: GenreSortField;
  genre_sort_asc: boolean;
  week_start: WeekStart;
}

class PrefsStore {
  ratingStyle = $state<RatingStyle>("heart");
  seekBarMode = $state<SeekBarMode>("waveform");
  fanartApiKey = $state<string>("");
  /** Which artist image types the enrichment batch retrieves (#1276).
   * Turning one off also hides already-fetched images of that type; local
   * files always show. */
  fanartFetchPhoto = $state<boolean>(true);
  fanartFetchLogo = $state<boolean>(true);
  fanartFetchBackground = $state<boolean>(true);
  /** Album image types, same rule (#1277). The cover only ever fills in for
   * an album with no cover of its own. */
  fanartFetchAlbumCover = $state<boolean>(true);
  fanartFetchDiscArt = $state<boolean>(true);
  /** Save album covers and artist portraits into music folders as cover.jpg/artist.jpg (#1274). Off by default. */
  saveArtworkToFolders = $state<boolean>(false);
  albumsViewMode = $state<CollectionViewMode>("cards");
  artistsViewMode = $state<CollectionViewMode>("cards");
  playlistsAutoViewMode = $state<CollectionViewMode>("cards");
  playlistsCustomViewMode = $state<CollectionViewMode>("cards");
  /** Collapses primary-genre cards down to compact header rows on the
   * Genres tab (mirrors the Albums/Artists cards-vs-rows toggle). */
  genreCardsViewMode = $state<CollectionViewMode>("cards");
  /** Sorts both the primary-genre cards and each card's own sub-genre chips —
   * display-only, doesn't touch the persisted drag-reorder sort_order. */
  genreSortField = $state<GenreSortField>("name");
  genreSortAsc = $state<boolean>(true);
  weekStart = $state<WeekStart>("sunday");
  /** Off by default — closing the window quits unless explicitly opted in. */
  minimizeToTray = $state<boolean>(false);
  /** Off by default; mirrors the OS's actual registration, queried fresh on init. */
  autostartEnabled = $state<boolean>(false);

  async init() {
    const prefs = await invoke<UiPreferences>("get_ui_preferences");
    this.ratingStyle = prefs.rating_style;
    this.seekBarMode = prefs.seekbar_mode;
    this.fanartApiKey = prefs.fanart_api_key;
    this.fanartFetchPhoto = prefs.fanart_fetch_photo;
    this.fanartFetchLogo = prefs.fanart_fetch_logo;
    this.fanartFetchBackground = prefs.fanart_fetch_background;
    this.fanartFetchAlbumCover = prefs.fanart_fetch_album_cover;
    this.fanartFetchDiscArt = prefs.fanart_fetch_disc_art;
    this.saveArtworkToFolders = prefs.save_artwork_to_folders;
    this.albumsViewMode = prefs.albums_view_mode;
    this.artistsViewMode = prefs.artists_view_mode;
    this.playlistsAutoViewMode = prefs.playlists_auto_view_mode;
    this.playlistsCustomViewMode = prefs.playlists_custom_view_mode;
    this.genreCardsViewMode = prefs.genre_cards_view_mode;
    this.genreSortField = prefs.genre_sort_field;
    this.genreSortAsc = prefs.genre_sort_asc;
    this.weekStart = prefs.week_start;
    this.minimizeToTray = await invoke<boolean>("get_minimize_to_tray_enabled");
    try {
      this.autostartEnabled = await invoke<boolean>("get_autostart_enabled");
    } catch (e) {
      console.error("Failed to read autostart state:", e);
    }
  }

  /** Persist the whole current state — fire-and-forget on the backend. */
  private save() {
    const prefs: UiPreferences = {
      rating_style: this.ratingStyle,
      seekbar_mode: this.seekBarMode,
      fanart_api_key: this.fanartApiKey,
      fanart_fetch_photo: this.fanartFetchPhoto,
      fanart_fetch_logo: this.fanartFetchLogo,
      fanart_fetch_background: this.fanartFetchBackground,
      fanart_fetch_album_cover: this.fanartFetchAlbumCover,
      fanart_fetch_disc_art: this.fanartFetchDiscArt,
      save_artwork_to_folders: this.saveArtworkToFolders,
      albums_view_mode: this.albumsViewMode,
      artists_view_mode: this.artistsViewMode,
      playlists_auto_view_mode: this.playlistsAutoViewMode,
      playlists_custom_view_mode: this.playlistsCustomViewMode,
      genre_view_mode: "genre",
      genre_cards_view_mode: this.genreCardsViewMode,
      genre_sort_field: this.genreSortField,
      genre_sort_asc: this.genreSortAsc,
      week_start: this.weekStart,
    };
    invoke("set_ui_preferences", { prefs });
  }

  setRatingStyle(style: RatingStyle) {
    this.ratingStyle = style;
    this.save();
  }

  setFanartApiKey(key: string) {
    this.fanartApiKey = key;
    this.save();
  }

  setFanartFetchPhoto(enabled: boolean) {
    this.fanartFetchPhoto = enabled;
    this.save();
  }

  setFanartFetchLogo(enabled: boolean) {
    this.fanartFetchLogo = enabled;
    this.save();
  }

  setFanartFetchBackground(enabled: boolean) {
    this.fanartFetchBackground = enabled;
    this.save();
  }

  setFanartFetchAlbumCover(enabled: boolean) {
    this.fanartFetchAlbumCover = enabled;
    this.save();
  }

  setFanartFetchDiscArt(enabled: boolean) {
    this.fanartFetchDiscArt = enabled;
    this.save();
  }

  setSaveArtworkToFolders(enabled: boolean) {
    this.saveArtworkToFolders = enabled;
    this.save();
  }

  toggleSeekBarMode() {
    this.seekBarMode = this.seekBarMode === "waveform" ? "bands" : "waveform";
    this.save();
  }

  setAlbumsViewMode(mode: CollectionViewMode) {
    this.albumsViewMode = mode;
    this.save();
  }

  setArtistsViewMode(mode: CollectionViewMode) {
    this.artistsViewMode = mode;
    this.save();
  }

  setPlaylistsAutoViewMode(mode: CollectionViewMode) {
    this.playlistsAutoViewMode = mode;
    this.save();
  }

  setPlaylistsCustomViewMode(mode: CollectionViewMode) {
    this.playlistsCustomViewMode = mode;
    this.save();
  }
  setGenreCardsViewMode(mode: CollectionViewMode) {
    this.genreCardsViewMode = mode;
    this.save();
  }

  setGenreSortField(field: GenreSortField) {
    this.genreSortField = field;
    this.save();
  }

  setGenreSortAsc(asc: boolean) {
    this.genreSortAsc = asc;
    this.save();
  }

  setWeekStart(start: WeekStart) {
    this.weekStart = start;
    this.save();
  }

  /** Not part of `save()` — persisted via its own dedicated command so the
   * backend's `tray.rs` close handler picks up the change immediately. */
  setMinimizeToTray(enabled: boolean) {
    this.minimizeToTray = enabled;
    invoke("set_minimize_to_tray_enabled", { enabled });
  }

  /** Proxies straight to the OS via the plugin, which can fail (permissions,
   * sandboxed install) — awaits the result and reverts the toggle rather than
   * assuming success. */
  async setAutostart(enabled: boolean) {
    const previous = this.autostartEnabled;
    this.autostartEnabled = enabled;
    try {
      await invoke("set_autostart_enabled", { enabled });
    } catch (e) {
      console.error("Failed to set autostart:", e);
      this.autostartEnabled = previous;
    }
  }
}

export const prefs = new PrefsStore();
