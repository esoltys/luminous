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

  /** Links (or with `null`, unlinks) the default library. Rejects with the
   * backend's reason when the folder or its file can't be used. */
  async set(path: string | null) {
    try {
      await invoke("set_default_library", { path });
    } finally {
      await this.refresh();
    }
  }

  private toast(path: string, message: string) {
    if (this.toasted === message) return;
    this.toasted = message;
    toastStore.show(
      i18n.t(
        "settings.defaultLibraryLoadFailed",
        { path, message },
        `Couldn't load the genre hierarchy from ${path}: ${message}`
      ),
      "error"
    );
  }
}

export const hierarchySidecarStore = new HierarchySidecarStore();
