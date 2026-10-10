import type { ActiveTab, ActiveSubTab, SettingsTab, AutoPlaylistRef } from "../stores/navigation.svelte";
import type { Song, PlaybackState } from "../types";
import type { Locale } from "../stores/i18n.svelte";

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
  to(tab: ActiveTab, subTab?: ActiveSubTab): Promise<void>;

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
  playlist(target: number | AutoPlaylistRef["kind"]): Promise<void>;

  /**
   * Opens the settings view on a specific section.
   */
  settings(section?: SettingsTab): Promise<void>;
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
  play(target: number | PlaySongSelector): Promise<Song>;

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
  status(): PlaybackState;
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
  setLocale(locale: Locale): Promise<void>;

  /**
   * Configures layout parameters (sidebar visibility/width, right panel visibility).
   */
  setLayout(layout: {
    sidebarOpen?: boolean;
    rightPanelOpen?: boolean;
    sidebarWidth?: number;
    miniplayer?: boolean;
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
}

/**
 * Primary scripting API exposed on window.__LUMINOUS_SCRIPT__.
 */
export interface LuminousScriptApi {
  readonly version: string;
  readonly navigate: ScriptNavigationApi;
  readonly playback: ScriptPlaybackApi;
  readonly appearance: ScriptAppearanceApi;
  readonly dialogs: ScriptDialogsApi;
  readonly wait: ScriptWaitApi;
}

declare global {
  interface Window {
    __LUMINOUS_SCRIPT__?: LuminousScriptApi;
  }
}
