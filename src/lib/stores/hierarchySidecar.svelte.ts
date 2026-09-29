import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { i18n } from "./i18n.svelte";
import { toastStore } from "./toast.svelte";

interface DefaultLibraryStatus {
  path: string | null;
  error: string | null;
}

interface SidecarError {
  path: string;
  message: string;
}

/**
 * The default library (#1312): the watched folder whose
 * `luminous-hierarchy.json` holds the genre and artist-tag hierarchies. The
 * backend owns loading, watching and writing the file; this store only shows
 * which folder is linked and surfaces a file that couldn't be loaded.
 */
class HierarchySidecarStore {
  path = $state<string | null>(null);
  error = $state<string | null>(null);
  /** Last error toasted, so the startup status and the startup event don't
   * both toast the same failure. */
  private toasted: string | null = null;

  async init() {
    await listen<SidecarError>("hierarchy-sidecar-error", ({ payload }) => {
      this.error = payload.message;
      this.toast(payload.path, payload.message);
    });
    // The backend links a lone watched folder on its own when folders are
    // added or removed.
    await listen("default-library-changed", () => this.refresh());
    // An error from the startup load can be emitted before the listener above
    // is attached — the status call reports it too.
    await this.refresh();
    if (this.path && this.error) this.toast(this.path, this.error);
  }

  async refresh() {
    const status = await invoke<DefaultLibraryStatus>("get_default_library");
    this.path = status?.path ?? null;
    this.error = status?.error ?? null;
    if (!this.error) this.toasted = null;
  }

  /** What to show while the linked file can't be loaded. The parser's details
   * stay in the backend log. */
  get errorText(): string | null {
    return this.path && this.error ? brokenText(this.path) : null;
  }

  /** Links (or with `null`, unlinks) the default library. Rejects with a
   * message saying why when the folder or its file can't be used. */
  async set(path: string | null) {
    try {
      await invoke("set_default_library", { path });
    } catch (code) {
      throw refusalText(String(code), path);
    } finally {
      await this.refresh();
    }
  }

  private toast(path: string, message: string) {
    if (this.toasted === message) return;
    this.toasted = message;
    toastStore.show(brokenText(path), "error");
  }
}

function brokenText(path: string) {
  return i18n.t(
    "settings.defaultLibraryFileBroken",
    { path },
    `The genre hierarchy file in ${path} is broken.`
  );
}

/** `code` is the backend's `LinkRefusal` code. */
function refusalText(code: string, path: string | null) {
  if (path && code === "broken") return brokenText(path);
  if (path && code === "unavailable") {
    return i18n.t("settings.defaultLibraryUnavailable", { path }, `${path} isn't available.`);
  }
  return i18n.t("settings.defaultLibraryChangeFailed", undefined, "Couldn't change the default library.");
}

export const hierarchySidecarStore = new HierarchySidecarStore();
