// The scripting contract: everything an external caller of window.__LUMINOUS_SCRIPT__
// needs, in one file with no imports so another repo can vendor it as is.
// Versioning policy (semver): adding a method bumps minor; removing or renaming one,
// changing its signature, or changing when it resolves bumps major.
// contract-history.json records each version's methods, and contract.test.ts checks
// the bumps against it and that the app-type copies below haven't drifted.
// Signature and resolve-condition changes are only caught in review.

/** Version of this contract. Callers pass their vendored copy to `connect()`. */
export const SCRIPT_API_VERSION = "1.2.0";

/** What `connect()` resolves with: the running app's version and every method it offers. */
export interface ScriptContract {
  version: string;
  /** Dotted method paths, e.g. "screens.setSort" or "dialogs.welcome.show". */
  capabilities: string[];
}

// Copies of app types, checked against the originals in contract.test.ts.
export type ScriptTab = "home" | "collection" | "playlists" | "settings" | "lyrics" | "stats" | "organize" | "help";
export type ScriptSubTab = "songs" | "albums" | "artists" | "genres";
export type ScriptSettingsTab = "general" | "system" | "sources" | "integrations" | "themes" | "equalizer" | "about";
export type ScriptAutoPlaylistKind =
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
  | "daypart";
/** A song-table column key. */
export type ScriptSongColumn =
  | "track"
  | "title"
  | "artist"
  | "album"
  | "album_artist"
  | "artist_tag"
  | "bitdepth"
  | "bitrate"
  | "bpm"
  | "channels"
  | "composer"
  | "filesize"
  | "format"
  | "genre"
  | "grouping"
  | "initial_key"
  | "musicbrainz_id"
  | "path"
  | "samplerate"
  | "year"
  | "originalyear"
  | "actions"
  | "added"
  | "duration"
  | "lastplayed"
  | "library"
  | "playcount"
  | "rating"
  | "skipcount";
/** A BCP 47 UI locale tag such as "fr-CA"; `setLocale` rejects tags the app doesn't ship. */
export type ScriptLocale = string;

/** The song fields the contract promises; the app's real songs carry more. */
export interface ScriptSong {
  id: number;
  title?: string;
  artist?: string;
  album?: string;
  album_artist?: string;
  track?: number;
  disc?: number;
  year?: number;
  genre?: string;
  length_nanosec?: number;
}

export type ScriptPlayState = "stopped" | "playing" | "paused";

/** The playback fields the contract promises; the app's real snapshot carries more. */
export interface ScriptPlaybackState {
  state: ScriptPlayState;
  current_song?: ScriptSong;
  playlist_id?: number;
  position_nanosec: number;
  volume: number;
}

/**
 * Filter or target criteria for playing a song via the scripting API.
 */
export interface PlaySongSelector {
  id?: number;
  title?: string;
  artist?: string;
}

/**
 * Navigation controller surface.
 * High-level intent-based navigation that resolves once view and state updates settle.
 */
export interface ScriptNavigationApi {
  /**
   * Navigates to a top-level tab and optional sub-tab.
   */
  to(tab: ScriptTab, subTab?: ScriptSubTab | "auto" | "custom"): Promise<void>;

  /**
   * Navigates to an album detail view by name, optionally focusing a specific song.
   */
  album(albumName: string, focusSongId?: number): Promise<void>;

  /**
   * Navigates to an artist detail view by name.
   */
  artist(artistName: string): Promise<void>;

  /**
   * Navigates to a playlist detail view by ID or auto-playlist kind.
   */
  playlist(target: number | ScriptAutoPlaylistKind): Promise<void>;

  /**
   * Opens the settings view on a specific section.
   */
  settings(section?: ScriptSettingsTab): Promise<void>;
}

/**
 * Playback controller surface.
 * Provides intent-based playback methods backed by real backend event synchronization.
 */
export interface ScriptPlaybackApi {
  /**
   * Plays a song by ID or by title/artist search selector.
   * Resolves when the backend confirms playback state is 'playing' for that song.
   */
  play(target: number | PlaySongSelector): Promise<ScriptSong>;

  /**
   * Pauses active playback, resolving when paused.
   */
  pause(): Promise<void>;

  /**
   * Resumes playback, resolving when playing.
   */
  resume(): Promise<void>;

  /**
   * Seeks to a position in seconds, resolving once the position is registered.
   */
  seek(positionSeconds: number): Promise<void>;

  /**
   * Returns current playback state snapshot.
   */
  status(): ScriptPlaybackState;
}

/**
 * Appearance and layout controller surface.
 */
export interface ScriptAppearanceApi {
  /**
   * Applies active theme by ID (e.g. 'system', 'dynamic-artwork', 'ruby-red').
   */
  setTheme(themeId: string): Promise<void>;

  /**
   * Configures color scheme override.
   */
  setColorScheme(scheme: "light" | "dark" | "system"): Promise<void>;

  /**
   * Updates the active UI locale.
   */
  setLocale(locale: ScriptLocale): Promise<void>;

  /**
   * Configures layout parameters (sidebar visibility/width, right panel visibility).
   */
  setLayout(layout: {
    sidebarOpen?: boolean;
    rightPanelOpen?: boolean;
    sidebarWidth?: number;
    miniplayer?: boolean;
    immersive?: boolean;
  }): Promise<void>;
}

/**
 * Dialogs, modals, and overlays controller surface.
 */
export interface ScriptDialogsApi {
  /**
   * Opens the Keyboard Shortcuts modal.
   */
  openShortcuts(): Promise<void>;

  /**
   * Opens the single-song tag editor modal for the given song ID.
   */
  openTagEditor(songId: number): Promise<void>;

  /**
   * Closes any open top-level modal or dialog.
   */
  closeAll(): Promise<void>;

  /**
   * Dismisses every toast on screen, whatever language its text is in.
   */
  dismissToasts(): Promise<void>;

  /**
   * First-run welcome screen controls.
   */
  welcome: {
    show(): void;
    dismiss(): void;
  };

  /**
   * Guided walkthrough tour controls.
   */
  walkthrough: {
    start(): void;
    stop(): void;
    next(): void;
  };
}

/**
 * Event-driven synchronization and timing helpers.
 */
export interface ScriptWaitApi {
  /**
   * Waits for a real Tauri backend event to fire with an optional predicate.
   * Automatically unregisters listener on receipt or timeout.
   */
  forEvent<T = unknown>(
    eventName: string,
    predicate?: (payload: T) => boolean,
    timeoutMs?: number
  ): Promise<T>;

  /**
   * Waits until a synchronous predicate over store state returns true.
   */
  forState(predicate: () => boolean, timeoutMs?: number): Promise<void>;

  /**
   * Waits for Svelte microtasks, ticks, and layout renders to settle.
   */
  settled(): Promise<void>;

  /**
   * Resolves only once all visual inputs for the current view have settled:
   * 1. In-flight navigation and Svelte DOM renders have flushed
   * 2. Waveform or frequency band data for the current track is loaded and drawn
   * 3. Dynamic Artwork colors are extracted and CSS variables applied
   * 4. All visible artwork and avatar <img> elements are loaded and decoded
   * 5. Double animation frames have painted to the compositor
   *
   * @param timeoutMs Maximum time to wait before rejecting (default: 10000ms).
   */
  visualsSettled(timeoutMs?: number): Promise<void>;
}

/** A grid that can show its items as cards or rows. */
export type ScriptViewSurface = "albums" | "artists" | "playlistsAuto" | "playlistsCustom" | "genres" | "pinned" | "artistReleases";

/**
 * Persistent view preferences scenes need to set without clicking through menus.
 * Every method is idempotent: setting what is already set succeeds without a change.
 */
export interface ScriptViewApi {
  /** Sets the cards/rows mode of several grids at once; surfaces not listed are left alone. */
  setViewModes(modes: Partial<Record<ScriptViewSurface, "cards" | "rows">>): Promise<void>;
  setSeekbarMode(mode: "waveform" | "bands"): Promise<void>;
  /** Shows or hides a song-table column by its key (e.g. "initial_key"). */
  setColumnVisible(column: ScriptSongColumn, visible: boolean): Promise<void>;
  /** Switches the Organize view to a custom template pattern. */
  setOrganizeTemplate(template: string): Promise<void>;
  /** Opens the smart-playlist builder on Playlists → Custom, pre-filled with rules. */
  openSmartPlaylistBuilder(rules?: Array<{ field: string; op: string; value: string }>): Promise<void>;
}

/**
 * Component-local states reachable only through the component that owns them.
 * Each rejects with a hint if its screen isn't showing.
 */
export interface ScriptScreensApi {
  /** Sorts the Collection songs, albums or artists view, as its sort menu does. Needs the Collection tab on screen. */
  setSort(surface: "songs" | "albums" | "artists", field: string, ascending: boolean): Promise<void>;
  /** Types into the top search box as a user would, opening its dropdown. */
  search(query: string): Promise<void>;
  /** Opens the album tag editor from the open album's detail view. */
  openAlbumEditor(): Promise<void>;
  setEqualizerMode(mode: "graphic10" | "parametric"): Promise<void>;
  openEqualizerPresetMenu(): Promise<void>;
  /** Loads a built-in or user equalizer preset by name, as the preset picker does. */
  selectEqualizerPreset(name: string): Promise<void>;
}

/**
 * Target options for window sizing.
 */
export interface ScriptWindowSizeOptions {
  /** Target inner width in logical pixels. */
  width?: number;
  /** Target inner height in logical pixels. */
  height?: number;
  /** Target effective display/zoom scale factor (e.g. 1.0 for 100%, 2.0 for 200%). */
  scale?: number;
}

/**
 * Window geometry and display scale controller surface.
 */
export interface ScriptWindowApi {
  /**
   * Resizes the window to the requested inner dimensions in logical pixels,
   * accounting for OS window decoration offsets where supported, and optionally
   * adjusts the webview zoom scale factor.
   *
   * Calling setSize allows captures to guarantee identical rendered pixel dimensions
   * independent of host display scaling.
   */
  setSize(options: ScriptWindowSizeOptions): Promise<void>;
}

/**
 * Primary scripting API exposed on window.__LUMINOUS_SCRIPT__.
 */
export interface LuminousScriptApi {
  readonly version: string;
  /**
   * The handshake: call once before anything else with the contract version the caller
   * was written against (caret semantics: "1.2" accepts 1.2.0 up to, not including, 2.0.0)
   * and any methods it can't do without. Rejects with a message naming the needed range,
   * the running version and the missing methods; has no side effects.
   */
  connect(requires: string, capabilities?: string[]): Promise<ScriptContract>;
  readonly navigate: ScriptNavigationApi;
  readonly playback: ScriptPlaybackApi;
  readonly appearance: ScriptAppearanceApi;
  readonly dialogs: ScriptDialogsApi;
  readonly view: ScriptViewApi;
  readonly screens: ScriptScreensApi;
  readonly wait: ScriptWaitApi;
  readonly window: ScriptWindowApi;
}

declare global {
  interface Window {
    __LUMINOUS_SCRIPT__?: LuminousScriptApi;
  }
}
