import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { i18n } from "../stores/i18n.svelte";
import { isWindows, isLinux } from "../platform";

/** "Show in File Explorer", "Show in Finder" or "Show in file manager". */
export function revealLabel(): string {
  return i18n.t(
    isWindows ? "files.revealWindows" : isLinux ? "files.revealLinux" : "files.revealMac",
  );
}

/** Shows a local file or folder selected in the system file manager. */
export async function reveal(location: string) {
  try {
    await revealItemInDir(location);
  } catch (err) {
    console.error("Failed to show a file in the file manager:", err);
  }
}
