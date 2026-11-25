import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import type { StationListFile, SupportedExtensions } from "../types";
import { createEventScope } from "../utils/eventScope";
import { i18n } from "./i18n.svelte";
import { playerStore } from "./player.svelte";
import { playlistsStore } from "./playlists.svelte";
import { toastStore } from "./toast.svelte";

/**
 * Owns every "open external paths" workflow: file/folder dialogs, drag-and-drop
 * handling, OS file-association / CLI startup files, `open-file-request` events,
 * and station-list import offers (#2326).
 */
export class OpenerStore {
  /** Station-list playlist files the user just opened or dropped, waiting
   * for them to confirm the import; `StationListImportDialog` shows while
   * this is non-empty (#2279). */
  pendingStationLists = $state<StationListFile[]>([]);
  /** Whether an OS file/folder drag is currently hovering the window. */
  isDragActive = $state(false);
  /** Whether Shift is held during an active drag (append vs. replace Queue). */
  isShiftHeld = $state(false);

  private events = createEventScope(import.meta.hot);
  private extensionsPromise: Promise<SupportedExtensions> | null = null;
  private shiftPollInterval: ReturnType<typeof setInterval> | undefined;

  constructor() {
    void this.init();
  }

  private async init() {
    try {
      // A file association (or `luminous <file>`) launched a second instance
      // while this one was already running; the backend forwards the paths
      // here instead of spawning a separate window.
      await this.events.listen<string[]>("open-file-request", (event) => {
        void this.openAndPlay(event.payload);
      });

      const startupFile = await invoke<string | null>("get_startup_file");
      if (startupFile) {
        await this.openAndPlay([startupFile]);
      }
    } catch (err) {
      console.error("Failed to initialize OpenerStore:", err);
    }
  }

  /** Fetches the backend's authoritative audio and playlist extension lists
   * once and caches them for subsequent file-open dialogs. */
  getSupportedExtensions(): Promise<SupportedExtensions> {
    if (!this.extensionsPromise) {
      this.extensionsPromise = invoke<SupportedExtensions>("get_supported_extensions").catch(
        (err) => {
          this.extensionsPromise = null;
          throw err;
        },
      );
    }
    return this.extensionsPromise;
  }

  /** Holds station-list files from an open/drop outcome for the import
   * confirmation, and reports whether there were any. */
  private offerStationLists(lists: StationListFile[] | undefined): boolean {
    if (!lists?.length) return false;
    this.pendingStationLists = lists;
    return true;
  }

  /** Takes the station lists awaiting confirmation, clearing the offer —
   * called by the dialog's Import (to import them) and Cancel (to drop them). */
  takePendingStationLists(): StationListFile[] {
    const lists = this.pendingStationLists;
    this.pendingStationLists = [];
    return lists;
  }

  async openAndPlay(paths: string[]) {
    playerStore.completedSession = null;
    const outcome = await invoke<{
      played: number;
      skipped: number;
      station_lists: StationListFile[];
    }>("open_and_play", { paths });
    const offeredStations = this.offerStationLists(outcome.station_lists);
    if (outcome.played === 0) {
      if (!offeredStations) {
        toastStore.show(
          i18n.t("playerBar.openNothingPlayable", {}, "No supported audio files found to play."),
          "error",
        );
      }
      return outcome;
    }
    if (outcome.skipped > 0) {
      toastStore.show(i18n.plural("playerBar.tracksSkippedToast", outcome.skipped), "error");
    }
    await playlistsStore.refreshPlaylists();
    const queuePl = await playlistsStore.requireQueue();
    playerStore.activeContextName = "Queue";
    await playlistsStore.selectPlaylist(queuePl.id);
    return outcome;
  }

  /** Drag-and-dropped (or otherwise opened) paths, appended to the end of the
   * Queue instead of replacing it — the "hold Shift to append" counterpart to
   * `openAndPlay`. Playback is left untouched. */
  async addPathsToQueue(paths: string[]) {
    playerStore.completedSession = null;
    const outcome = await invoke<{
      added: number;
      skipped: number;
      station_lists: StationListFile[];
    }>("add_paths_to_queue", { paths });
    const offeredStations = this.offerStationLists(outcome.station_lists);
    if (outcome.added === 0) {
      if (!offeredStations) {
        toastStore.show(
          i18n.t("dragDrop.nothingToAdd", {}, "No supported audio files found to add."),
          "error",
        );
      }
      return outcome;
    }
    if (outcome.skipped > 0) {
      toastStore.show(i18n.plural("playerBar.tracksSkippedToast", outcome.skipped), "error");
    }
    await playlistsStore.refreshPlaylists();
    const qPl = playlistsStore.queuePlaylist;
    if (qPl && playlistsStore.activePlaylistId === qPl.id) {
      await playlistsStore.selectPlaylist(qPl.id);
    }
    return outcome;
  }

  async openFileDialog() {
    try {
      const { audio, playlists } = await this.getSupportedExtensions();
      const selected = await open({
        multiple: true,
        directory: false,
        title: i18n.t("topNav.openFilesTitle", {}, "Open Audio Files or Playlists"),
        filters: [
          {
            name: i18n.t("topNav.fileFilterSupported"),
            extensions: [...audio, ...playlists],
          },
          {
            name: i18n.t("topNav.fileFilterAudio"),
            extensions: [...audio],
          },
          {
            name: i18n.t("topNav.fileFilterPlaylists"),
            extensions: [...playlists],
          },
        ],
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await this.openAndPlay(paths);
        }
      }
    } catch (err) {
      console.error("Failed to open files/playlists:", err);
    }
  }

  /** Plays every audio file under the picked folder(s) (recursively) without
   * adding the folder to the watched library — the button counterpart to
   * dropping a folder on the window. */
  async openFolderDialog() {
    try {
      const selected = await open({
        multiple: true,
        directory: true,
        title: i18n.t("emptyLibrary.playFolder"),
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await this.openAndPlay(paths);
        }
      }
    } catch (err) {
      console.error("Failed to open folder:", err);
    }
  }

  private async refreshShiftHeld() {
    try {
      this.isShiftHeld = await invoke<boolean>("is_shift_key_held");
    } catch (e) {
      console.warn("Failed to query Shift key state:", e);
    }
  }

  private stopShiftPolling() {
    if (this.shiftPollInterval) {
      clearInterval(this.shiftPollInterval);
      this.shiftPollInterval = undefined;
    }
  }

  private async handleFilesDropped(paths: string[]) {
    const append = await invoke<boolean>("is_shift_key_held").catch(() => false);
    if (append) {
      const outcome = await this.addPathsToQueue(paths);
      if (outcome.added > 0) {
        toastStore.show(i18n.plural("dragDrop.addedSongs", outcome.added), "success");
      }
    } else {
      const outcome = await this.openAndPlay(paths);
      if (outcome.played > 0) {
        toastStore.show(i18n.plural("dragDrop.playingSongs", outcome.played), "success");
      }
    }
  }

  /**
   * Attaches the native webview drag-and-drop listener and polls physical Shift
   * key state while a drag hovers (an OS file drag never gives the target window
   * keyboard focus, so DOM keydown/keyup listeners cannot fire). Returns a
   * cleanup function for `onMount`.
   */
  attachDragDrop(): () => void {
    let disposed = false;
    let dragDropUnlisten: (() => void) | undefined;

    // Must be getCurrentWebview(), not getCurrentWindow(): the Rust side
    // (manager/webview.rs's emit_to_webview) emits drag-drop events scoped to
    // an EventTarget::Webview, so Window.onDragDropEvent() silently never fires.
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          this.isDragActive = false;
          this.stopShiftPolling();
          void this.handleFilesDropped(event.payload.paths);
        } else if (event.payload.type === "leave") {
          this.isDragActive = false;
          this.stopShiftPolling();
        } else if (!this.isDragActive) {
          this.isDragActive = true;
          void this.refreshShiftHeld();
          this.shiftPollInterval = setInterval(() => void this.refreshShiftHeld(), 150);
        }
      })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          dragDropUnlisten = unlisten;
        }
      })
      .catch((e) => {
        console.warn("Failed to attach drag-and-drop listener:", e);
      });

    return () => {
      disposed = true;
      this.isDragActive = false;
      this.isShiftHeld = false;
      this.stopShiftPolling();
      dragDropUnlisten?.();
    };
  }

  /** Stops listening to backend events and clears any active drag polling. */
  dispose() {
    this.events.dispose();
    this.stopShiftPolling();
  }
}

export const openerStore = new OpenerStore();
