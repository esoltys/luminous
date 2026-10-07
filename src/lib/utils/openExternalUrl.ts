import { openUrl } from "@tauri-apps/plugin-opener";

/** Opens `url` in the system browser, falling back to a new window outside
 * Tauri (where `openUrl` rejects). Imported statically so the opener is
 * called synchronously from the click that asked for it. */
export async function openExternalUrl(url: string) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank");
  }
}
