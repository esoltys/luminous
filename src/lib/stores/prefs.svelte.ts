import { invoke } from "@tauri-apps/api/core";
import { createEventScope, type EventScope } from "../utils/eventScope";
import type { AudioDeviceInfo, AudioOutputConfig, AudioPipelineInfo } from "../types";

export type RatingStyle = "heart" | "stars" | "both";
type SeekBarMode = "waveform" | "bands";
export type CollectionViewMode = "cards" | "rows";
export type GenreSortField = "name" | "count";
export type WeekStart = "sunday" | "monday";
/** The families of generated auto-playlists the user can switch off (#2131); the
 * backend's `auto_playlists_<family>` preferences, which it enforces in its sync. */
export type AutoPlaylistFamily =
  "genre" | "decade" | "artist_tag" | "bpm" | "language" | "country" | "daypart";

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
  detect_tempo_and_key: boolean;
  verify_audio_integrity_and_provenance: boolean;
  airplay_enabled: boolean;
  airplay_prompt_dismissed: boolean;
  auto_playlists_genre: boolean;
  auto_playlists_decade: boolean;
  auto_playlists_artist_tag: boolean;
  auto_playlists_bpm: boolean;
  auto_playlists_language: boolean;
  auto_playlists_country: boolean;
  auto_playlists_daypart: boolean;
  albums_view_mode: CollectionViewMode;
  artists_view_mode: CollectionViewMode;
  playlists_auto_view_mode: CollectionViewMode;
  playlists_custom_view_mode: CollectionViewMode;
  genre_view_mode: string;
  genre_cards_view_mode: CollectionViewMode;
  pinned_view_mode: CollectionViewMode;
  artist_releases_view_mode: CollectionViewMode;
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
  /** Estimate BPM and key for songs missing those tags by analysing audio in the background (#1943). Off by default. */
  detectTempoAndKey = $state<boolean>(false);
  /** Verify audio file integrity and lossless provenance in the background (#2189). Off by default. */
  verifyAudioIntegrityAndProvenance = $state<boolean>(false);
  /** Show "On the radio now" in the right panel; sends artist and title to TunePing (#1998). Off by default. */
  airplayEnabled = $state<boolean>(false);
  /** The user chose "Not now" on the "On the radio now" consent card, so it stays hidden. */
  airplayPromptDismissed = $state<boolean>(false);
  /** Which families of auto-playlists the library keeps; all on by default. */
  autoPlaylists = $state<Record<AutoPlaylistFamily, boolean>>({
    genre: true,
    decade: true,
    artist_tag: true,
    bpm: true,
    language: true,
    country: true,
    daypart: true,
  });
  albumsViewMode = $state<CollectionViewMode>("cards");
  artistsViewMode = $state<CollectionViewMode>("cards");
  playlistsAutoViewMode = $state<CollectionViewMode>("cards");
  playlistsCustomViewMode = $state<CollectionViewMode>("cards");
  /** Collapses primary-genre cards down to compact header rows on the
   * Genres tab (mirrors the Albums/Artists cards-vs-rows toggle). */
  genreCardsViewMode = $state<CollectionViewMode>("cards");
  pinnedViewMode = $state<CollectionViewMode>("cards");
  /** Shared by every release and playlist section of the artist view. */
  artistReleasesViewMode = $state<CollectionViewMode>("cards");
  /** Sorts both the primary-genre cards and each card's own sub-genre chips —
   * display-only, doesn't touch the persisted drag-reorder sort_order. */
  genreSortField = $state<GenreSortField>("name");
  genreSortAsc = $state<boolean>(true);
  weekStart = $state<WeekStart>("sunday");
  /** Off by default — closing the window quits unless explicitly opted in. */
  minimizeToTray = $state<boolean>(false);
  /** Off by default; mirrors the OS's actual registration, queried fresh on init. */
  autostartEnabled = $state<boolean>(false);
  /** Online/Offline master toggle (#1398). Off means Luminous makes no requests
   * to third-party internet services. The backend owns the value and enforces
   * it; this mirrors it (loaded in `init()`, kept current by `online-mode-changed`)
   * so the UI can hide online-only actions. */
  onlineEnabled = $state<boolean>(true);
  audioOutputDevices = $state<AudioDeviceInfo[]>([]);
  audioOutputConfig = $state<AudioOutputConfig>({ backend: "cpal", device_id: null });
  audioOutputLoading = $state<boolean>(false);
  private events = createEventScope(import.meta.hot);
  private outputScope: EventScope | null = null;
  /** The open output last reported, to reload the device list only when it moves. */
  private lastOutputKey: string | null = null;

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
    this.detectTempoAndKey = prefs.detect_tempo_and_key ?? false;
    this.verifyAudioIntegrityAndProvenance = prefs.verify_audio_integrity_and_provenance ?? false;
    this.airplayEnabled = prefs.airplay_enabled ?? false;
    this.airplayPromptDismissed = prefs.airplay_prompt_dismissed ?? false;
    this.autoPlaylists = {
      genre: prefs.auto_playlists_genre ?? true,
      decade: prefs.auto_playlists_decade ?? true,
      artist_tag: prefs.auto_playlists_artist_tag ?? true,
      bpm: prefs.auto_playlists_bpm ?? true,
      language: prefs.auto_playlists_language ?? true,
      country: prefs.auto_playlists_country ?? true,
      daypart: prefs.auto_playlists_daypart ?? true,
    };
    this.albumsViewMode = prefs.albums_view_mode;
    this.artistsViewMode = prefs.artists_view_mode;
    this.playlistsAutoViewMode = prefs.playlists_auto_view_mode;
    this.playlistsCustomViewMode = prefs.playlists_custom_view_mode;
    this.genreCardsViewMode = prefs.genre_cards_view_mode;
    this.pinnedViewMode = prefs.pinned_view_mode;
    this.artistReleasesViewMode = prefs.artist_releases_view_mode;
    this.genreSortField = prefs.genre_sort_field;
    this.genreSortAsc = prefs.genre_sort_asc;
    this.weekStart = prefs.week_start;
    try {
      this.onlineEnabled = await invoke<boolean>("is_context_enrichment_enabled");
      await this.events.listen<boolean>("online-mode-changed", (event) => {
        this.onlineEnabled = event.payload;
      });
    } catch (e) {
      console.error("Failed to read online mode:", e);
    }
    this.minimizeToTray = await invoke<boolean>("get_minimize_to_tray_enabled");
    try {
      this.autostartEnabled = await invoke<boolean>("get_autostart_enabled");
    } catch (e) {
      console.error("Failed to read autostart state:", e);
    }
    await this.refreshAudioDevices();
    await this.followOutputChanges();
  }

  /** Reloads the device list whenever the open output moves (a fallback, an
   * unplugged or returning device), so Settings never lists a stale set. */
  private async followOutputChanges() {
    this.outputScope?.dispose();
    this.outputScope = this.events.createChild();
    try {
      await Promise.all([
        this.outputScope.listen("audio-output-fallback", () => void this.refreshAudioDevices()),
        this.outputScope.listen<AudioPipelineInfo | null>("audio-pipeline-changed", (event) => {
          const p = event.payload;
          const key = p ? `${p.output_backend}|${p.output_device_name}` : null;
          if (key === this.lastOutputKey) return;
          this.lastOutputKey = key;
          void this.refreshAudioDevices();
        }),
      ]);
    } catch (e) {
      console.error("Failed to follow audio output changes:", e);
    }
  }

  /** Persist the whole current state — fire-and-forget on the backend. */
  private save(): Promise<unknown> {
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
      detect_tempo_and_key: this.detectTempoAndKey,
      verify_audio_integrity_and_provenance: this.verifyAudioIntegrityAndProvenance,
      airplay_enabled: this.airplayEnabled,
      airplay_prompt_dismissed: this.airplayPromptDismissed,
      auto_playlists_genre: this.autoPlaylists.genre,
      auto_playlists_decade: this.autoPlaylists.decade,
      auto_playlists_artist_tag: this.autoPlaylists.artist_tag,
      auto_playlists_bpm: this.autoPlaylists.bpm,
      auto_playlists_language: this.autoPlaylists.language,
      auto_playlists_country: this.autoPlaylists.country,
      auto_playlists_daypart: this.autoPlaylists.daypart,
      albums_view_mode: this.albumsViewMode,
      artists_view_mode: this.artistsViewMode,
      playlists_auto_view_mode: this.playlistsAutoViewMode,
      playlists_custom_view_mode: this.playlistsCustomViewMode,
      genre_view_mode: "genre",
      genre_cards_view_mode: this.genreCardsViewMode,
      pinned_view_mode: this.pinnedViewMode,
      artist_releases_view_mode: this.artistReleasesViewMode,
      genre_sort_field: this.genreSortField,
      genre_sort_asc: this.genreSortAsc,
      week_start: this.weekStart,
    };
    return invoke("set_ui_preferences", { prefs });
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

  setDetectTempoAndKey(enabled: boolean) {
    this.detectTempoAndKey = enabled;
    this.save();
  }

  setVerifyAudioIntegrityAndProvenance(enabled: boolean) {
    this.verifyAudioIntegrityAndProvenance = enabled;
    this.save();
  }

  /** Turns "On the radio now" on or off; either answer retires the consent card. */
  setAirplayEnabled(enabled: boolean) {
    this.airplayEnabled = enabled;
    if (!enabled) this.airplayPromptDismissed = true;
    this.save();
  }

  /** "Not now" on the consent card: hide the card without turning the section on. */
  dismissAirplayPrompt() {
    this.airplayPromptDismissed = true;
    this.save();
  }

  /** Switches an auto-playlist family on or off. The backend sync does the
   * pruning or regeneration, so this persists first and then runs it; the
   * sync's `playlists-changed` event refreshes every open view. */
  async setAutoPlaylistFamily(family: AutoPlaylistFamily, enabled: boolean) {
    this.autoPlaylists[family] = enabled;
    try {
      await this.save();
      await invoke("sync_all_auto_playlists");
    } catch (e) {
      console.error("Failed to apply auto-playlist setting:", e);
    }
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

  setPinnedViewMode(mode: CollectionViewMode) {
    this.pinnedViewMode = mode;
    this.save();
  }

  setArtistReleasesViewMode(mode: CollectionViewMode) {
    this.artistReleasesViewMode = mode;
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

  /** Persisted and enforced by the backend, which also suspends Discord and
   * ListenBrainz and emits `online-mode-changed`. Reverts if the write fails. */
  async setOnlineEnabled(enabled: boolean) {
    const previous = this.onlineEnabled;
    this.onlineEnabled = enabled;
    try {
      await invoke("set_online_enabled", { enabled });
    } catch (e) {
      console.error("Failed to set online mode:", e);
      this.onlineEnabled = previous;
    }
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

  async refreshAudioDevices() {
    try {
      const [devices, config] = await Promise.all([
        invoke<AudioDeviceInfo[]>("get_audio_output_devices"),
        invoke<AudioOutputConfig>("get_audio_output_settings"),
      ]);
      this.audioOutputDevices = devices;
      this.audioOutputConfig = config;
    } catch (e) {
      console.error("Failed to load audio devices:", e);
    }
  }

  async setAudioOutput(config: AudioOutputConfig) {
    this.audioOutputLoading = true;
    try {
      await invoke("set_audio_output_settings", { settings: config });
      this.audioOutputConfig = config;
    } catch (e) {
      console.error("Failed to set audio output settings:", e);
      throw e;
    } finally {
      this.audioOutputLoading = false;
    }
  }

  /** Reopen the output with the current settings, e.g. to retry exclusive
   *  mode once the app holding the device has let it go. */
  async reapplyAudioOutput() {
    await this.setAudioOutput({ ...this.audioOutputConfig });
  }

  async openAsioControlPanel(deviceId?: string | null) {
    try {
      await invoke("open_asio_control_panel", { deviceId: deviceId ?? null });
    } catch (e) {
      console.error("Failed to open ASIO control panel:", e);
      throw e;
    }
  }

  /** Stops listening to backend events. */
  dispose() {
    this.events.dispose();
  }
}

export const prefs = new PrefsStore();
