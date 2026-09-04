import { invoke } from "@tauri-apps/api/core";

export type RatingStyle = "heart" | "stars";
type SeekBarMode = "waveform" | "bands";
export type CollectionViewMode = "cards" | "rows";
export type GenreViewMode = "genre" | "tags";
export type GenreSortField = "name" | "count";

/** Shape of the backend's UiPreferences struct — the schema (keys, domains,
 * defaults) lives in Rust (commands/settings.rs); this store just mirrors it. */
interface UiPreferences {
  rating_style: RatingStyle;
  seekbar_mode: SeekBarMode;
  acoustid_api_key: string;
  albums_view_mode: CollectionViewMode;
  artists_view_mode: CollectionViewMode;
  playlists_auto_view_mode: CollectionViewMode;
  playlists_custom_view_mode: CollectionViewMode;
  genre_view_mode: GenreViewMode;
  genre_cards_view_mode: CollectionViewMode;
  genre_sort_field: GenreSortField;
  genre_sort_asc: boolean;
}

class PrefsStore {
  ratingStyle = $state<RatingStyle>("heart");
  seekBarMode = $state<SeekBarMode>("waveform");
  acoustidApiKey = $state<string>("");
  albumsViewMode = $state<CollectionViewMode>("cards");
  artistsViewMode = $state<CollectionViewMode>("cards");
  playlistsAutoViewMode = $state<CollectionViewMode>("cards");
  playlistsCustomViewMode = $state<CollectionViewMode>("cards");
  genreViewMode = $state<GenreViewMode>("genre");
  /** Collapses primary-genre cards down to compact header rows on the
   * Genres tab (mirrors the Albums/Artists cards-vs-rows toggle). */
  genreCardsViewMode = $state<CollectionViewMode>("cards");
  /** Sorts both the primary-genre cards and each card's own sub-genre chips —
   * display-only, doesn't touch the persisted drag-reorder sort_order. */
  genreSortField = $state<GenreSortField>("name");
  genreSortAsc = $state<boolean>(true);
  /** Off by default — closing the window quits unless explicitly opted in. */
  minimizeToTray = $state<boolean>(false);
  /** Off by default; mirrors the OS's actual registration, queried fresh on init. */
  autostartEnabled = $state<boolean>(false);

  async init() {
    const prefs = await invoke<UiPreferences>("get_ui_preferences");
    this.ratingStyle = prefs.rating_style;
    this.seekBarMode = prefs.seekbar_mode;
    this.acoustidApiKey = prefs.acoustid_api_key;
    this.albumsViewMode = prefs.albums_view_mode;
    this.artistsViewMode = prefs.artists_view_mode;
    this.playlistsAutoViewMode = prefs.playlists_auto_view_mode;
    this.playlistsCustomViewMode = prefs.playlists_custom_view_mode;
    this.genreViewMode = prefs.genre_view_mode;
    this.genreCardsViewMode = prefs.genre_cards_view_mode;
    this.genreSortField = prefs.genre_sort_field;
    this.genreSortAsc = prefs.genre_sort_asc;
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
      acoustid_api_key: this.acoustidApiKey,
      albums_view_mode: this.albumsViewMode,
      artists_view_mode: this.artistsViewMode,
      playlists_auto_view_mode: this.playlistsAutoViewMode,
      playlists_custom_view_mode: this.playlistsCustomViewMode,
      genre_view_mode: this.genreViewMode,
      genre_cards_view_mode: this.genreCardsViewMode,
      genre_sort_field: this.genreSortField,
      genre_sort_asc: this.genreSortAsc,
    };
    invoke("set_ui_preferences", { prefs });
  }

  setRatingStyle(style: RatingStyle) {
    this.ratingStyle = style;
    this.save();
  }

  toggleSeekBarMode() {
    this.seekBarMode = this.seekBarMode === "waveform" ? "bands" : "waveform";
    this.save();
  }

  setAcoustidApiKey(key: string) {
    this.acoustidApiKey = key;
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

  setGenreViewMode(mode: GenreViewMode) {
    this.genreViewMode = mode;
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
