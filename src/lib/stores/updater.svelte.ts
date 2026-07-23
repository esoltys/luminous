import { invoke } from "@tauri-apps/api/core";

export interface InstallFormatInfo {
  format: string;
  human_name: string;
  supports_self_update: boolean;
}

export type CheckStatus = "idle" | "checking" | "available" | "up-to-date" | "error";

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
  releaseUrl = $state("");
  downloadUrl = $state("");
  errorMessage = $state<string | null>(null);

  private intervalTimer: ReturnType<typeof setInterval> | null = null;

  async init() {
    try {
      // 1. Load preferences
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings?.update_check_enabled === "true") {
        this.updateCheckEnabled = true;
      }
      if (settings?.update_auto_install === "true") {
        this.updateAutoInstall = true;
      }

      // 2. Fetch install format from backend
      try {
        const fmt = await invoke<InstallFormatInfo>("get_install_format");
        if (fmt) {
          this.installFormat = fmt;
        }
      } catch (err) {
        console.error("Failed to detect install format:", err);
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
  }

  startPeriodicCheck() {
    this.stopPeriodicCheck();
    // Check every 4 hours (14,400,000 ms) while app is running
    this.intervalTimer = setInterval(() => {
      if (this.updateCheckEnabled) {
        this.checkForUpdates();
      }
    }, 4 * 60 * 60 * 1000);
  }

  stopPeriodicCheck() {
    if (this.intervalTimer) {
      clearInterval(this.intervalTimer);
      this.intervalTimer = null;
    }
  }

  async checkForUpdates() {
    this.checkStatus = "checking";
    this.errorMessage = null;

    try {
      // Fetch latest release info from GitHub API
      const res = await fetch("https://api.github.com/repos/esoltys/luminous/releases/latest", {
        headers: { Accept: "application/vnd.github.v3+json" }
      });

      if (!res.ok) {
        throw new Error(`GitHub API returned HTTP ${res.status}`);
      }

      const data = await res.json();
      const latestTag = (data.tag_name || "").replace(/^v/, "");
      
      let currentVersion = "";
      try {
        const { getVersion } = await import("@tauri-apps/api/app");
        currentVersion = await getVersion();
      } catch {
        currentVersion = "0.90.0";
      }

      const isNewer = this.isVersionNewer(latestTag, currentVersion);

      if (isNewer) {
        this.updateAvailable = true;
        this.latestVersion = data.tag_name || `v${latestTag}`;
        this.releaseUrl = data.html_url || "https://github.com/esoltys/luminous/releases/latest";

        // Find matching download asset URL based on install format
        const assets: Array<{ name: string; browser_download_url: string }> = data.assets || [];
        const matchingAsset = this.findMatchingAsset(assets, this.installFormat.format);
        if (matchingAsset) {
          this.downloadUrl = matchingAsset.browser_download_url;
        } else {
          this.downloadUrl = this.releaseUrl;
        }

        this.checkStatus = "available";
      } else {
        this.updateAvailable = false;
        this.checkStatus = "up-to-date";
      }
    } catch (err: any) {
      console.warn("Update check failed:", err);
      this.checkStatus = "error";
      this.errorMessage = err?.message || "Failed to check for updates";
    }
  }

  private findMatchingAsset(
    assets: Array<{ name: string; browser_download_url: string }>,
    format: string
  ): { name: string; browser_download_url: string } | undefined {
    const extMap: Record<string, string[]> = {
      deb: [".deb"],
      rpm: [".rpm"],
      appimage: [".appimage"],
      windows_setup: ["-setup.exe", ".msi", ".exe"],
    };

    const exts = extMap[format] || [];
    for (const ext of exts) {
      const found = assets.find((a) => a.name.toLowerCase().endsWith(ext.toLowerCase()));
      if (found) return found;
    }
    return undefined;
  }

  private isVersionNewer(latest: string, current: string): boolean {
    if (!latest || !current) return false;
    const parse = (v: string) => v.split(".").map((n) => parseInt(n, 10) || 0);
    const l = parse(latest);
    const c = parse(current);
    for (let i = 0; i < Math.max(l.length, c.length); i++) {
      const numL = l[i] || 0;
      const numC = c[i] || 0;
      if (numL > numC) return true;
      if (numL < numC) return false;
    }
    return false;
  }
}

export const updaterStore = new UpdaterStore();
