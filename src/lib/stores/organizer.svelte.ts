import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";
import { TOAST_DURATION_MS } from "../constants";

interface OrganizeConfig {
  auto_organize: boolean;
  template: string;
  preset: string;
  destination_mode: string;
  custom_destination_dir: string;
  replace_spaces: boolean;
  ascii_only: boolean;
  clean_empty_dirs: boolean;
  move_extra_files: boolean;
}

export interface OrganizeConsequences {
  fileCount: number;
  folderCount: number;
  cleanEmptyDirs: boolean;
  moveExtraFiles: boolean;
}

/**
 * Computes the aggregate consequences of applying an organize operation from preview items.
 * Only items eligible to be moved (ok or missing_tag without collisions and with from_path != to_path)
 * are counted towards moving files and destination folders.
 */
export function computeOrganizeConsequences(
  items: Array<{ from_path?: string; to_path?: string; status?: string; error_message?: string | null }>,
  options: { cleanEmptyDirs: boolean; moveExtraFiles: boolean }
): OrganizeConsequences {
  const movingItems = items.filter((item) => {
    const s = String(item.status || "").toLowerCase();
    const canMove = s === "ok" || s === "missing_tag" || s === "missingtag";
    const hasCollision = s === "collision" || (s === "ok" && item.error_message);
    const fromPath = item.from_path ?? "";
    const toPath = item.to_path ?? "";
    return canMove && !hasCollision && fromPath.length > 0 && toPath.length > 0 && fromPath !== toPath;
  });

  const targetFolders = new Set<string>();
  for (const item of movingItems) {
    const toPath = item.to_path ?? "";
    const lastSep = Math.max(toPath.lastIndexOf("/"), toPath.lastIndexOf("\\"));
    if (lastSep > 0) {
      targetFolders.add(toPath.slice(0, lastSep));
    }
  }

  return {
    fileCount: movingItems.length,
    folderCount: targetFolders.size,
    cleanEmptyDirs: options.cleanEmptyDirs,
    moveExtraFiles: options.moveExtraFiles,
  };
}

interface AutoOrganizeResult {
  moved_count: number;
  duplicates_count: number;
  errors: string[];
}

const DEFAULT_ORGANIZE_TEMPLATE = "%albumartist/{%year - }{%album/}{%disc-}{%track }%title";

class OrganizerStore {
  autoOrganize = $state<boolean>(false);
  template = $state<string>(DEFAULT_ORGANIZE_TEMPLATE);
  preset = $state<string>("default");
  destinationMode = $state<"original" | "custom">("original");
  customDestinationDir = $state<string>("");
  replaceSpaces = $state<boolean>(false);
  asciiOnly = $state<boolean>(false);
  cleanEmptyDirs = $state<boolean>(true);
  moveExtraFiles = $state<boolean>(true);
  isLoaded = $state<boolean>(false);
  isDraftActive = $state<boolean>(false);

  private unlisten: UnlistenFn | null = null;
  private pendingMovedCount = 0;
  private debounceTimer: ReturnType<typeof setTimeout> | null = null;

  setDraftActive(active: boolean) {
    this.isDraftActive = active;
  }

  async init() {
    try {
      const config = await invoke<OrganizeConfig>("get_organize_config");
      this.applyConfig(config);
    } catch (err) {
      console.error("Failed to load organize config:", err);
    } finally {
      this.isLoaded = true;
    }

    if (!this.unlisten) {
      try {
        this.unlisten = await listen<AutoOrganizeResult>("auto-organize-result", (event) => {
          this.handleResult(event.payload);
        });
      } catch (err) {
        console.error("Failed to listen for auto-organize-result:", err);
      }
    }
  }

  applyConfig(config: OrganizeConfig) {
    this.autoOrganize = !!config.auto_organize;
    this.template = config.template || DEFAULT_ORGANIZE_TEMPLATE;
    this.preset = config.preset || "default";
    this.destinationMode = config.destination_mode === "custom" ? "custom" : "original";
    this.customDestinationDir = config.custom_destination_dir || "";
    this.replaceSpaces = !!config.replace_spaces;
    this.asciiOnly = !!config.ascii_only;
    this.cleanEmptyDirs = config.clean_empty_dirs !== false;
    this.moveExtraFiles = config.move_extra_files !== false;
  }

  getConfig(): OrganizeConfig {
    return {
      auto_organize: this.autoOrganize,
      template: this.template,
      preset: this.preset,
      destination_mode: this.destinationMode,
      custom_destination_dir: this.customDestinationDir,
      replace_spaces: this.replaceSpaces,
      ascii_only: this.asciiOnly,
      clean_empty_dirs: this.cleanEmptyDirs,
      move_extra_files: this.moveExtraFiles,
    };
  }

  async saveConfig(): Promise<void> {
    try {
      await invoke("set_organize_config", { config: this.getConfig() });
    } catch (err) {
      console.error("Failed to persist organize config:", err);
    }
  }

  async setAutoOrganize(enabled: boolean): Promise<void> {
    this.autoOrganize = enabled;
    await this.saveConfig();
  }

  async updateConfig(partial: Partial<OrganizeConfig>): Promise<void> {
    if (partial.auto_organize !== undefined) this.autoOrganize = partial.auto_organize;
    if (partial.template !== undefined) this.template = partial.template;
    if (partial.preset !== undefined) this.preset = partial.preset;
    if (partial.destination_mode !== undefined) {
      this.destinationMode = partial.destination_mode === "custom" ? "custom" : "original";
    }
    if (partial.custom_destination_dir !== undefined) {
      this.customDestinationDir = partial.custom_destination_dir;
    }
    if (partial.replace_spaces !== undefined) this.replaceSpaces = partial.replace_spaces;
    if (partial.ascii_only !== undefined) this.asciiOnly = partial.ascii_only;
    if (partial.clean_empty_dirs !== undefined) this.cleanEmptyDirs = partial.clean_empty_dirs;
    if (partial.move_extra_files !== undefined) this.moveExtraFiles = partial.move_extra_files;
    await this.saveConfig();
  }

  handleResult(result: AutoOrganizeResult) {
    const { moved_count, duplicates_count, errors } = result;

    // 1. Success notification: batched and non-sticky (auto-dismisses in TOAST_DURATION_MS = 4000)
    if (moved_count > 0) {
      this.pendingMovedCount += moved_count;
      if (this.debounceTimer) clearTimeout(this.debounceTimer);
      this.debounceTimer = setTimeout(() => {
        const count = this.pendingMovedCount;
        this.pendingMovedCount = 0;
        this.debounceTimer = null;
        if (count > 0) {
          const text = i18n.plural("organizer.autoOrganizeSuccess", count);
          toastStore.show(text, "success", TOAST_DURATION_MS);
        }
      }, 1200);
    }

    // 2. Duplicates detected: sticky warning (remains until manually dismissed)
    if (duplicates_count > 0) {
      const text = i18n.plural("organizer.toastDuplicatesDetected", duplicates_count);
      toastStore.show(text, "warning");
    }

    // 3. File errors: sticky error (remains until manually dismissed)
    if (errors && errors.length > 0) {
      const text = i18n.t("organizer.toastErrors", { count: errors.length });
      toastStore.show(text, "error");
    }
  }

  destroy() {
    if (this.unlisten) {
      this.unlisten();
      this.unlisten = null;
    }
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
      this.debounceTimer = null;
    }
  }
}

export const organizeStore = new OrganizerStore();
