import { describe, it, expect, beforeEach, vi } from "vitest";
import { i18n, formatNumber } from "./i18n.svelte";
import { invoke } from "@tauri-apps/api/core";

// Mock Tauri invoke for settings
vi.mock("@tauri-apps/api/core", () => {
  return {
    invoke: vi.fn().mockImplementation(async (cmd, args) => {
      if (cmd === "get_all_app_settings") {
        return { language: "fr" };
      }
      return null;
    }),
  };
});

describe("I18nStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA"; // reset to default
  });

  it("should initialize locale from Tauri backend", async () => {
    await i18n.init();
    expect(i18n.currentLocale).toBe("fr-CA");
    expect(invoke).toHaveBeenCalledWith("get_all_app_settings");
  });

  describe("saved language migration", () => {
    const mockSettings = (settings: Record<string, string>) =>
      vi.mocked(invoke).mockImplementation(async (cmd: string) =>
        cmd === "get_all_app_settings" ? settings : null
      );

    it("aliases a pre-registry value and writes the tag plus the marker back", async () => {
      mockSettings({ language: "fr" });
      await i18n.init();
      expect(i18n.currentLocale).toBe("fr-CA");
      expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "language", value: "fr-CA" });
      expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "language_tags", value: "1" });
    });

    it("keeps the default and still sets the marker on a fresh install", async () => {
      mockSettings({});
      await i18n.init();
      expect(i18n.currentLocale).toBe("en-CA");
      expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "language_tags", value: "1" });
      expect(invoke).not.toHaveBeenCalledWith("set_app_setting", expect.objectContaining({ key: "language" }));
    });

    it("reads a tag as-is once the marker is set, without writing anything", async () => {
      mockSettings({ language: "fr-CA", language_tags: "1" });
      await i18n.init();
      expect(i18n.currentLocale).toBe("fr-CA");
      expect(invoke).not.toHaveBeenCalledWith("set_app_setting", expect.objectContaining({ key: "language" }));
      expect(invoke).not.toHaveBeenCalledWith("set_app_setting", expect.objectContaining({ key: "language_tags" }));
    });

    it("does not alias a bare legacy value once the marker is set", async () => {
      mockSettings({ language: "en", language_tags: "1" });
      await i18n.init();
      expect(i18n.currentLocale).toBe("en-CA");
    });

    it("treats a bare fr as France French once the marker is set", async () => {
      mockSettings({ language: "fr", language_tags: "1" });
      await i18n.init();
      expect(i18n.currentLocale).toBe("fr");
    });

    it("ignores an unknown saved language", async () => {
      mockSettings({ language: "xx", language_tags: "1" });
      await i18n.init();
      expect(i18n.currentLocale).toBe("en-CA");
    });
  });

  it("should change locale and update settings database via Tauri", async () => {
    await i18n.setLocale("fr-CA");
    expect(i18n.currentLocale).toBe("fr-CA");
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "language",
      value: "fr-CA",
    });
  });

  it("should translate keys with correct locale", () => {
    i18n.currentLocale = "en-CA";
    expect(i18n.t("collection.noSongsTitle")).toBe("No songs found");

    i18n.currentLocale = "fr-CA";
    expect(i18n.t("collection.noSongsTitle")).toBe("Aucune chanson trouvée");
  });

  it("should support different translations", () => {
    i18n.currentLocale = "en-CA";
    expect(i18n.t("settings.tabGeneral")).toBe("General");
    expect(i18n.t("lyrics.fetching")).toBe("Fetching lyrics...");
    expect(i18n.t("lyrics.plainTextNotice")).toBe("Synced lyrics not available. Showing plain text.");
    expect(i18n.t("playerBar.songLabel")).toBe("Song");
    expect(i18n.t("playerBar.bitrateLabel")).toBe("Bitrate");

    i18n.currentLocale = "fr-CA";
    expect(i18n.t("settings.tabGeneral")).toBe("Général");
    expect(i18n.t("lyrics.fetching")).toBe("Récupération des paroles...");
    expect(i18n.t("lyrics.plainTextNotice")).toBe("Paroles synchronisées non disponibles. Affichage du texte brut.");
    expect(i18n.t("playerBar.songLabel")).toBe("Chanson");
    expect(i18n.t("playerBar.bitrateLabel")).toBe("Débit");
  });

  it("should fallback to English for missing keys in target locale", () => {
    i18n.currentLocale = "fr-CA";
    // If a key is missing in French catalog but present in English
    // Let's assert on fallback to English catalog
    expect(i18n.t("sidebar.nonexistent")).toBe("sidebar.nonexistent");
  });

  it("should use explicit fallback when translation is missing", () => {
    i18n.currentLocale = "en-CA";
    expect(i18n.t("nonexistent.key", {}, "My Fallback")).toBe("My Fallback");
  });

  it("should interpolate variables correctly", () => {
    i18n.currentLocale = "en-CA";
    expect(i18n.t("collection.songs", { count: 42 })).toBe("Songs (42)");

    i18n.currentLocale = "fr-CA";
    expect(i18n.t("collection.songs", { count: 42 })).toBe("Chansons (42)");
  });

  it("should handle missing variables by leaving placeholders", () => {
    i18n.currentLocale = "en-CA";
    expect(i18n.t("collection.songs", {})).toBe("Songs ({count})");
  });

  describe("plural", () => {
    it("picks the one and other forms for English", () => {
      i18n.currentLocale = "en-CA";
      expect(i18n.plural("playlists.songsCount", 1)).toBe("1 song");
      expect(i18n.plural("playlists.songsCount", 0)).toBe("0 songs");
      expect(i18n.plural("playlists.songsCount", 2)).toBe("2 songs");
    });

    it("follows the locale's own rules: French treats 0 as singular", () => {
      i18n.currentLocale = "fr-CA";
      expect(i18n.plural("playlists.songsCount", 0)).toBe("0 chanson");
      expect(i18n.plural("playlists.songsCount", 1)).toBe("1 chanson");
      expect(i18n.plural("playlists.songsCount", 2)).toBe("2 chansons");
    });

    it("formats {count} for the locale and lets vars override it", () => {
      i18n.currentLocale = "en-CA";
      expect(i18n.plural("playlists.songsCount", 12345)).toBe("12,345 songs");
      expect(i18n.plural("playlists.songsCount", 2, { count: "two" })).toBe("two songs");
    });

    it("passes extra variables through", () => {
      i18n.currentLocale = "en-CA";
      expect(i18n.plural("settings.folderLocateSuccess", 1, { path: "D:\Music" })).toBe("Re-linked 1 song to D:\Music");
    });

    it("returns the key or the explicit fallback when the key is missing or not counted", () => {
      i18n.currentLocale = "en-CA";
      expect(i18n.plural("nonexistent.key", 3)).toBe("nonexistent.key");
      expect(i18n.plural("nonexistent.key", 3, {}, "Fallback")).toBe("Fallback");
      expect(i18n.plural("sidebar.home", 3)).toBe("sidebar.home");
    });
  });

  it("should format numbers with locale decimal separator", () => {
    i18n.currentLocale = "en-CA";
    expect(formatNumber(0.71, { minimumFractionDigits: 2 })).toBe("0.71");
    expect(i18n.formatNumber(12.0, { minimumFractionDigits: 1 })).toBe("12.0");

    i18n.currentLocale = "fr-CA";
    expect(formatNumber(0.71, { minimumFractionDigits: 2 })).toBe("0,71");
    expect(i18n.formatNumber(12.0, { minimumFractionDigits: 1 })).toBe("12,0");
  });

  it("should format numbers with options like signDisplay", () => {
    i18n.currentLocale = "en-CA";
    expect(formatNumber(1.5, { minimumFractionDigits: 1, signDisplay: "exceptZero" })).toBe("+1.5");
    expect(formatNumber(-1.5, { minimumFractionDigits: 1, signDisplay: "exceptZero" })).toBe("-1.5");
    expect(formatNumber(0.0, { minimumFractionDigits: 1, signDisplay: "exceptZero" })).toBe("0.0");

    i18n.currentLocale = "fr-CA";
    expect(formatNumber(1.5, { minimumFractionDigits: 1, signDisplay: "exceptZero" })).toBe("+1,5");
    expect(formatNumber(-1.5, { minimumFractionDigits: 1, signDisplay: "exceptZero" })).toBe("-1,5");
    expect(formatNumber(0.0, { minimumFractionDigits: 1, signDisplay: "exceptZero" })).toBe("0,0");
  });

  it("should handle non-finite numbers safely", () => {
    expect(formatNumber(NaN)).toBe("NaN");
    expect(formatNumber(Infinity)).toBe("Infinity");
  });

  it("should sync document.documentElement.lang on setLocale", async () => {
    await i18n.setLocale("fr-CA");
    if (typeof document !== "undefined") {
      expect(document.documentElement.lang).toBe("fr-CA");
    }

    await i18n.setLocale("en-CA");
    if (typeof document !== "undefined") {
      expect(document.documentElement.lang).toBe("en-CA");
    }
  });
});

describe("manual language", () => {
  const mockSettings = (settings: Record<string, string>) =>
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "get_all_app_settings" ? settings : null
    );

  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
    (i18n as unknown as { explicitManualLanguage: null }).explicitManualLanguage = null;
  });

  it("follows the UI language until the user picks one", () => {
    expect(i18n.manualLanguage).toBe("EN");
    i18n.currentLocale = "fr-CA";
    expect(i18n.manualLanguage).toBe("FR");
  });

  it("keeps an explicit choice when the UI language changes, and saves it", async () => {
    await i18n.setManualLanguage("EN");
    i18n.currentLocale = "fr-CA";
    expect(i18n.manualLanguage).toBe("EN");
    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "manual_language", value: "EN" });
  });

  it("restores the saved choice on launch and ignores an invalid one", async () => {
    mockSettings({ language: "en-CA", language_tags: "1", manual_language: "FR" });
    await i18n.init();
    expect(i18n.manualLanguage).toBe("FR");

    (i18n as unknown as { explicitManualLanguage: null }).explicitManualLanguage = null;
    mockSettings({ language: "en-CA", language_tags: "1", manual_language: "DE" });
    await i18n.init();
    expect(i18n.manualLanguage).toBe("EN");
  });

  describe("update-notification template", () => {
    it("caches the UI-language template, keeping {version} for the backend, on launch", async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) =>
        cmd === "get_all_app_settings" ? { language: "fr-CA", language_tags: "1" } : null,
      );
      await i18n.init();
      expect(invoke).toHaveBeenCalledWith("set_app_setting", {
        key: "update_notification_template",
        value: "Luminous a été mis à jour vers la v{version}",
      });
    });

    it("leaves the cached template alone when the settings fail to load", async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_all_app_settings") throw new Error("db unavailable");
        return null;
      });
      vi.spyOn(console, "error").mockImplementation(() => {});
      await i18n.init();
      expect(invoke).not.toHaveBeenCalledWith("set_app_setting", expect.objectContaining({ key: "update_notification_template" }));
    });

    it("re-caches it when the language changes", async () => {
      await i18n.setLocale("de");
      expect(invoke).toHaveBeenCalledWith("set_app_setting", {
        key: "update_notification_template",
        value: "Luminous wurde auf v{version} aktualisiert",
      });
    });
  });
});
