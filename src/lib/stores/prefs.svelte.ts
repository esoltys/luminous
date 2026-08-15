import { invoke } from "@tauri-apps/api/core";

export type RatingStyle = "heart" | "stars";
export type SeekBarMode = "waveform" | "bands";
export type CollectionViewMode = "cards" | "rows";

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
}

class PrefsStore {
  ratingStyle = $state<RatingStyle>("heart");
  seekBarMode = $state<SeekBarMode>("waveform");
  acoustidApiKey = $state<string>("");
  albumsViewMode = $state<CollectionViewMode>("cards");
  artistsViewMode = $state<CollectionViewMode>("cards");
  playlistsAutoViewMode = $state<CollectionViewMode>("cards");
  playlistsCustomViewMode = $state<CollectionViewMode>("cards");
  /** Off by default — closing the window quits unless explicitly opted in. */
  minimizeToTray = $state<boolean>(false);

  async init() {
    const prefs = await invoke<UiPreferences>("get_ui_preferences");
    this.ratingStyle = prefs.rating_style;
    this.seekBarMode = prefs.seekbar_mode;
    this.acoustidApiKey = prefs.acoustid_api_key;
    this.albumsViewMode = prefs.albums_view_mode;
    this.artistsViewMode = prefs.artists_view_mode;
    this.playlistsAutoViewMode = prefs.playlists_auto_view_mode;
    this.playlistsCustomViewMode = prefs.playlists_custom_view_mode;
    this.minimizeToTray = await invoke<boolean>("get_minimize_to_tray_enabled");
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

  /** Not part of `save()` — persisted via its own dedicated command so the
   * backend's `tray.rs` close handler picks up the change immediately. */
  setMinimizeToTray(enabled: boolean) {
    this.minimizeToTray = enabled;
    invoke("set_minimize_to_tray_enabled", { enabled });
  }
}

export const prefs = new PrefsStore();
