import { invoke } from "@tauri-apps/api/core";
import { check as checkForUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

interface InstallFormatInfo {
  format: string;
  human_name: string;
  supports_self_update: boolean;
}

type CheckStatus = "idle" | "checking" | "available" | "up-to-date" | "error";
type InstallStatus = "idle" | "downloading" | "ready-to-restart" | "error";
type UpdatePolicy = "never" | "notify" | "auto";
interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

const UPDATE_CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000; // 4 hours
const RELEASES_URL = "https://github.com/esoltys/luminous/releases/latest";
export const MICROSOFT_STORE_URL = "https://apps.microsoft.com/detail/9PNQ2NFSQ7XW";

// Tauri command rejections often surface as a raw string (from a Rust `Err(String)`),
// not an `Error` instance — `err.message` on a string is always `undefined`.
function extractErrorMessage(err: unknown, fallback: string): string {
  if (typeof err === "string" && err.trim()) return err;
  if (err instanceof Error && err.message) return err.message;
  return fallback;
}

class UpdaterStore {
  updateCheckEnabled = $state(false);
  updateAutoInstall = $state(false);

  installFormat = $state<InstallFormatInfo>({
    format: "unknown",
    human_name: "Desktop Application",
    supports_self_update: false,
  });

  checkStatus = $state<CheckStatus>("idle");
  updateAvailable = $state(false);
  latestVersion = $state("");
  releaseUrl = $state(RELEASES_URL);
  errorMessage = $state<string | null>(null);
  lastCheckedAt = $state<number | null>(null);

  get updatePolicy(): UpdatePolicy {
    if (!this.updateCheckEnabled) return "never";
    return this.updateAutoInstall ? "auto" : "notify";
  }

  // deb/rpm and MSIX installs are all updated by something other than us —
  // apt/dnf or the Microsoft Store — and GitHub releases aren't even the
  // same artifact stream. The check/notify/auto-install UI is meaningless
  // there, so callers use this to swap in "managed by X" messaging instead
  // of a control panel with nothing it can do.
  get externallyManagedFormat(): "deb" | "rpm" | "msix" | null {
    switch (this.installFormat.format) {
      case "deb":
      case "rpm":
      case "msix":
        return this.installFormat.format;
      default:
        return null;
    }
  }

  get isExternallyManaged(): boolean {
    return this.externallyManagedFormat !== null;
  }

  installStatus = $state<InstallStatus>("idle");
  downloadProgress = $state<DownloadProgress | null>(null);

  private intervalTimer: ReturnType<typeof setInterval> | null = null;
  private pendingUpdate: Update | null = null;
  private initialized = false;

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      // 1. Fetch install format from backend
      try {
        const fmt = await invoke<InstallFormatInfo>("get_install_format");
        if (fmt) {
          this.installFormat = fmt;
        }
      } catch (err) {
        console.error("Failed to detect install format:", err);
      }

      // Externally-managed installs never check or install updates themselves —
      // stored preferences are irrelevant, and there's nothing to poll for.
      if (this.isExternallyManaged) {
        this.updateCheckEnabled = false;
        return;
      }

      // 2. Load preferences
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings?.update_check_enabled === "false") {
        this.updateCheckEnabled = false;
      } else {
        this.updateCheckEnabled = true;
      }
      if (settings?.update_auto_install === "true") {
        this.updateAutoInstall = true;
      }

      // 3. Perform check & setup interval if enabled
      if (this.updateCheckEnabled) {
        this.checkForUpdates();
        this.startPeriodicCheck();
      }
    } catch (e) {
      console.error("Failed to initialize updater store:", e);
    }
  }

  async setUpdateCheckEnabled(enabled: boolean) {
    this.updateCheckEnabled = enabled;
    if (!enabled) {
      this.updateAutoInstall = false;
      await invoke("set_app_setting", { key: "update_auto_install", value: "false" });
      this.stopPeriodicCheck();
    } else {
      this.startPeriodicCheck();
      this.checkForUpdates();
    }
    try {
      await invoke("set_app_setting", { key: "update_check_enabled", value: String(enabled) });
    } catch (e) {
      console.error("Failed to save update_check_enabled setting:", e);
    }
  }

  async setUpdateAutoInstall(enabled: boolean) {
    this.updateAutoInstall = enabled;
    try {
      await invoke("set_app_setting", { key: "update_auto_install", value: String(enabled) });
    } catch (e) {
      console.error("Failed to save update_auto_install setting:", e);
    }
    if (enabled && this.updateAvailable && this.installFormat.supports_self_update && this.installStatus === "idle") {
      this.downloadAndInstall();
    }
  }

  async setUpdatePolicy(policy: UpdatePolicy) {
    if (policy === "never") {
      if (this.updateCheckEnabled) await this.setUpdateCheckEnabled(false);
      return;
    }
    if (!this.updateCheckEnabled) await this.setUpdateCheckEnabled(true);
    const wantsAutoInstall = policy === "auto";
    if (this.updateAutoInstall !== wantsAutoInstall) await this.setUpdateAutoInstall(wantsAutoInstall);
  }

  startPeriodicCheck() {
    this.stopPeriodicCheck();
    this.intervalTimer = setInterval(() => {
      if (this.updateCheckEnabled) {
        this.checkForUpdates();
      }
    }, UPDATE_CHECK_INTERVAL_MS);
  }

  stopPeriodicCheck() {
    if (this.intervalTimer) {
      clearInterval(this.intervalTimer);
      this.intervalTimer = null;
    }
  }

  async checkForUpdates() {
    if (this.isExternallyManaged) return;

    this.checkStatus = "checking";
    this.errorMessage = null;
    this.installStatus = "idle";
    this.downloadProgress = null;

    if (this.pendingUpdate) {
      await this.pendingUpdate.close();
      this.pendingUpdate = null;
    }

    try {
      const update = await checkForUpdate();
      this.lastCheckedAt = Date.now();

      if (update) {
        this.pendingUpdate = update;
        this.updateAvailable = true;
        this.latestVersion = `v${update.version}`;
        this.releaseUrl = `https://github.com/esoltys/luminous/releases/tag/v${update.version}`;
        this.checkStatus = "available";

        if (this.updateAutoInstall && this.installFormat.supports_self_update) {
          this.downloadAndInstall();
        }
      } else {
        this.updateAvailable = false;
        this.checkStatus = "up-to-date";
      }
    } catch (err: unknown) {
      console.warn("Update check failed:", err);
      this.checkStatus = "error";
      this.errorMessage = extractErrorMessage(err, "No response from the update server");
    }
  }

  async downloadAndInstall() {
    if (!this.pendingUpdate || !this.installFormat.supports_self_update || this.installStatus === "downloading") {
      return;
    }

    this.installStatus = "downloading";
    this.downloadProgress = { downloaded: 0, total: null };
    this.errorMessage = null;

    try {
      await this.pendingUpdate.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            this.downloadProgress = { downloaded: 0, total: event.data.contentLength ?? null };
            break;
          case "Progress":
            if (this.downloadProgress) {
              this.downloadProgress = {
                downloaded: this.downloadProgress.downloaded + event.data.chunkLength,
                total: this.downloadProgress.total,
              };
            }
            break;
          case "Finished":
            break;
        }
      });
      this.installStatus = "ready-to-restart";
    } catch (err: unknown) {
      console.error("Failed to download and install update:", err);
      this.installStatus = "error";
      this.errorMessage = extractErrorMessage(err, "Download failed");
    }
  }

  async restartNow() {
    try {
      await relaunch();
    } catch (err) {
      console.error("Failed to restart for update:", err);
    }
  }
}

export const updaterStore = new UpdaterStore();
