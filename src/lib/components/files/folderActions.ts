import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { playerStore } from "../../stores/player.svelte";
import { playlistsStore } from "../../stores/playlists.svelte";
import { devicesStore } from "../../stores/devices.svelte";
import { toastStore } from "../../stores/toast.svelte";
import { i18n } from "../../stores/i18n.svelte";
import { deviceName } from "../../utils/devices";
import { isWindows, isLinux } from "../../platform";
import type { ConnectedDevice, FileSource, Song } from "../../types";

/**
 * What the Files view does to a folder of a source (#1841, #1884): the
 * header buttons act on the open folder, the folder-row menu on a child
 * folder, both through here. A folder is `(source, path)`, names from the
 * source's root; `name` is what toasts and the queue's context call it.
 * Every action reports its own outcome in a toast and never throws.
 */

/** The library songs in a folder and below it, in layout order. */
function songsIn(source: FileSource, path: string[]): Promise<Song[]> {
  return invoke<Song[]>("get_source_songs", { source, path });
}

/** Plays a folder's songs, from `startSongId` when given (else the first). */
export async function playFolder(source: FileSource, path: string[], name: string, startSongId?: number) {
  try {
    const songs = await songsIn(source, path);
    if (songs.length === 0 && startSongId === undefined) {
      toastStore.show(i18n.t("files.nothingToPlay"), "info");
      return;
    }
    const ids = songs.length > 0 ? songs.map((s) => s.id) : [startSongId!];
    const start = Math.max(0, ids.indexOf(startSongId ?? ids[0]));
    await playerStore.playSongs(ids, start, undefined, undefined, name);
  } catch (err) {
    console.error("Failed to play folder:", err);
    toastStore.show(i18n.t("files.playFailed"), "error");
  }
}

export async function queueFolder(source: FileSource, path: string[], name: string) {
  try {
    const songs = await songsIn(source, path);
    if (songs.length === 0) {
      toastStore.show(i18n.t("files.nothingToPlay"), "info");
      return;
    }
    await playlistsStore.addSongsToQueue(songs.map((s) => s.id));
    toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }));
  } catch (err) {
    console.error("Failed to queue folder:", err);
    toastStore.show(i18n.t("files.queueFailed"), "error");
  }
}

/** A new custom playlist named after the folder, holding its songs. */
export async function playlistFromFolder(source: FileSource, path: string[], name: string) {
  try {
    const songs = await songsIn(source, path);
    if (songs.length === 0) {
      toastStore.show(i18n.t("files.nothingToPlay"), "info");
      return;
    }
    const playlist = await playlistsStore.createPlaylist(name);
    await playlistsStore.addSongsToPlaylist(playlist.id, songs.map((s) => s.id));
    toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: playlist.name }));
  } catch (err) {
    console.error("Failed to create a playlist from a folder:", err);
    toastStore.show(i18n.t("files.playlistFailed"), "error");
  }
}

/** Looks for new and changed files in the folder; only folder sources scan. */
export async function scanFolder(source: FileSource, path: string[]) {
  try {
    await invoke("scan_source_folder", { source, path });
  } catch (err) {
    console.error("Failed to scan folder:", err);
    toastStore.show(i18n.t("files.scanFailed"), "error");
  }
}

/** Adds the folder at `location`, a local path, to a device's sync selection. */
export async function addFolderToDeviceSync(device: ConnectedDevice, location: string, name: string) {
  const deviceLabel = deviceName(device);
  try {
    if (!(await devicesStore.addFolderToSync(device.id, location))) {
      toastStore.show(i18n.t("files.alreadySyncing", { folder: name, device: deviceLabel }), "info");
      return;
    }
    toastStore.show(i18n.t("files.addedToSync", { folder: name, device: deviceLabel }), "success");
  } catch (err) {
    console.error("Failed to add folder to device sync:", err);
    toastStore.show(i18n.t("files.addToSyncFailed", { device: deviceLabel }), "error");
  }
}

/** "Show in File Explorer", "Show in Finder" or "Show in file manager". */
export function revealLabel(): string {
  return i18n.t(isWindows ? "files.revealWindows" : isLinux ? "files.revealLinux" : "files.revealMac");
}

/** Shows a local file or folder selected in the system file manager. */
export async function reveal(location: string) {
  try {
    await revealItemInDir(location);
  } catch (err) {
    console.error("Failed to show a file in the file manager:", err);
  }
}

/** Copies local paths to the clipboard, one per line. */
export async function copyPaths(locations: string[]) {
  try {
    await navigator.clipboard.writeText(locations.join("\n"));
    toastStore.show(i18n.plural("files.pathsCopied", locations.length), "success", 3000);
  } catch (err) {
    console.error("Failed to copy paths:", err);
    toastStore.show(i18n.t("files.copyFailed"), "error");
  }
}
