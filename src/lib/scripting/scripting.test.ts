import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createScriptingApi, installScriptingApi } from "./index";
import { registerDialogHostControls } from "./dialogs";
import { registerScreenHook } from "./screens";
import { prefs } from "../stores/prefs.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playerStore } from "../stores/player.svelte";
import { themeStore } from "../stores/theme.svelte";
import { i18n } from "../stores/i18n.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import { toastStore } from "../stores/toast.svelte";
import { welcomeStore } from "../stores/welcome.svelte";
import { walkthroughStore } from "../stores/walkthrough.svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("In-app scripting API", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    delete window.__LUMINOUS_SCRIPT__;
  });

  afterEach(() => {
    delete window.__LUMINOUS_SCRIPT__;
  });

  describe("Assembly and Installation", () => {
    it("creates an API instance with expected deep controllers and version", () => {
      const api = createScriptingApi();
      expect(api.version).toBe("1.0.0");
      expect(api.navigate).toBeDefined();
      expect(api.playback).toBeDefined();
      expect(api.appearance).toBeDefined();
      expect(api.dialogs).toBeDefined();
      expect(api.wait).toBeDefined();
    });

    it("installs onto window.__LUMINOUS_SCRIPT__", () => {
      expect(window.__LUMINOUS_SCRIPT__).toBeUndefined();
      const api = installScriptingApi();
      expect(window.__LUMINOUS_SCRIPT__).toBe(api);
    });
  });

  describe("Navigation controller", () => {
    it("navigates to tab and subTab and clears detail selections", async () => {
      const api = createScriptingApi();
      navigationStore.selectedAlbumName = "Old Album";
      await api.navigate.to("collection", "albums");

      expect(navigationStore.activeTab).toBe("collection");
      expect(navigationStore.activeSubTab).toBe("albums");
      expect(navigationStore.selectedAlbumName).toBeNull();
    });

    it("navigates to album by name", async () => {
      const api = createScriptingApi();
      await api.navigate.album("Evermore", 42);

      expect(navigationStore.activeTab).toBe("collection");
      expect(navigationStore.activeSubTab).toBe("albums");
      expect(navigationStore.selectedAlbumName).toBe("Evermore");
      expect(navigationStore.pendingFocusSongId).toBe(42);
    });

    it("navigates to artist by name", async () => {
      const api = createScriptingApi();
      await api.navigate.artist("Cannons");

      expect(navigationStore.activeTab).toBe("collection");
      expect(navigationStore.activeSubTab).toBe("artists");
      expect(navigationStore.selectedArtistName).toBe("Cannons");
    });

    it("navigates to playlist by ID and auto-playlist by kind", async () => {
      const api = createScriptingApi();
      await api.navigate.playlist(12);

      expect(navigationStore.activeTab).toBe("playlists");
      expect(navigationStore.selectedPlaylistId).toBe(12);

      await api.navigate.playlist("favourites");
      expect(navigationStore.selectedAutoPlaylist?.kind).toBe("favourites");
    });

    it("navigates to settings section", async () => {
      const api = createScriptingApi();
      await api.navigate.settings("equalizer");

      expect(navigationStore.activeTab).toBe("settings");
      expect(navigationStore.settingsSubTab).toBe("equalizer");
    });
  });

  describe("Playback controller", () => {
    it("plays a song by ID and confirms playback", async () => {
      const api = createScriptingApi();
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "play_song") {
          playerStore.state = "playing";
          playerStore.currentSong = {
            id: 101,
            title: "Golden Hour",
            artist: "JVKE",
            source: "local_file",
            filetype: "MP3",
          } as any;
          return;
        }
        return null;
      });

      const song = await api.playback.play(101);
      expect(invoke).toHaveBeenCalledWith("play_song", { songId: 101 });
      expect(song.id).toBe(101);
      expect(song.title).toBe("Golden Hour");
    });

    it("searches and plays a song by title and artist selector", async () => {
      const api = createScriptingApi();
      const mockSong = {
        id: 202,
        title: "Fire for You",
        artist: "Cannons",
        source: "local_file",
        filetype: "FLAC",
      } as any;

      vi.mocked(invoke).mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "search_songs") {
          return [mockSong];
        }
        if (cmd === "play_song") {
          playerStore.state = "playing";
          playerStore.currentSong = mockSong;
          return;
        }
        return null;
      });

      const song = await api.playback.play({ title: "Fire for You", artist: "Cannons" });
      expect(invoke).toHaveBeenCalledWith("search_songs", { query: "Fire for You", limit: 100 });
      expect(invoke).toHaveBeenCalledWith("play_song", { songId: 202 });
      expect(song.id).toBe(202);
    });

    it("pauses, resumes, and seeks playback", async () => {
      const api = createScriptingApi();
      playerStore.state = "playing";

      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "pause_playback") {
          playerStore.state = "paused";
        } else if (cmd === "resume_playback") {
          playerStore.state = "playing";
        }
      });

      await api.playback.pause();
      expect(invoke).toHaveBeenCalledWith("pause_playback");
      expect(playerStore.state).toBe("paused");

      await api.playback.resume();
      expect(invoke).toHaveBeenCalledWith("resume_playback");
      expect(playerStore.state).toBe("playing");

      await api.playback.seek(45.5);
      expect(invoke).toHaveBeenCalledWith("seek_to", { positionNanosec: 45500000000 });
    });

    it("returns a snapshot of playback status", () => {
      const api = createScriptingApi();
      playerStore.state = "playing";
      playerStore.volume = 0.85;

      const status = api.playback.status();
      expect(status.state).toBe("playing");
      expect(status.volume).toBe(0.85);
    });
  });

  describe("Appearance controller", () => {
    it("sets theme, color scheme, and locale", async () => {
      const api = createScriptingApi();

      const setThemeSpy = vi.spyOn(themeStore, "setTheme").mockImplementation(async (id: string) => {
        themeStore.activeThemeId = id;
      });
      const setColorSchemeSpy = vi.spyOn(themeStore, "setColorSchemeMode").mockImplementation(async (mode) => {
        themeStore.colorSchemeMode = mode;
      });
      const setLocaleSpy = vi.spyOn(i18n, "setLocale").mockImplementation(async (locale) => {
        i18n.currentLocale = locale;
      });

      await api.appearance.setTheme("dynamic-artwork");
      expect(setThemeSpy).toHaveBeenCalledWith("dynamic-artwork");
      expect(themeStore.activeThemeId).toBe("dynamic-artwork");

      await api.appearance.setColorScheme("light");
      expect(setColorSchemeSpy).toHaveBeenCalledWith("light");
      expect(themeStore.colorSchemeMode).toBe("light");

      await api.appearance.setLocale("fr-CA");
      expect(setLocaleSpy).toHaveBeenCalledWith("fr-CA");
      expect(i18n.currentLocale).toBe("fr-CA");
    });

    it("sets window layout dimensions and panel toggles", async () => {
      const api = createScriptingApi();

      await api.appearance.setLayout({
        sidebarOpen: false,
        rightPanelOpen: true,
        sidebarWidth: 280,
      });

      expect(windowLayoutStore.sidebarOpen).toBe(false);
      expect(windowLayoutStore.rightPanelOpen).toBe(true);
      expect(windowLayoutStore.sidebarWidth).toBe(280);
    });

    it("toggles immersive mode only when it differs from the requested state", async () => {
      const api = createScriptingApi();
      windowLayoutStore.immersiveMode = false;

      await api.appearance.setLayout({ immersive: true });
      expect(windowLayoutStore.immersiveMode).toBe(true);

      await api.appearance.setLayout({ immersive: true });
      expect(windowLayoutStore.immersiveMode).toBe(true);

      await api.appearance.setLayout({ immersive: false });
      expect(windowLayoutStore.immersiveMode).toBe(false);
    });
  });

  describe("Dialogs controller", () => {
    it("dismisses every toast regardless of its text", async () => {
      const api = createScriptingApi();
      toastStore.show("Hello", "info");
      toastStore.show("Bonjour", "info");
      expect(toastStore.messages.length).toBe(2);

      await api.dialogs.dismissToasts();
      expect(toastStore.messages.length).toBe(0);
    });

    it("integrates with registered dialog host controls", async () => {
      const api = createScriptingApi();
      let shortcutsOpen = false;
      let tagEditorSong: number | null = null;

      registerDialogHostControls({
        openShortcuts: () => { shortcutsOpen = true; },
        openTagEditor: (id) => { tagEditorSong = id; },
        closeAll: () => {
          shortcutsOpen = false;
          tagEditorSong = null;
        },
        isShortcutsOpen: () => shortcutsOpen,
        isTagEditorOpen: () => tagEditorSong !== null,
      });

      await api.dialogs.openShortcuts();
      expect(shortcutsOpen).toBe(true);

      await api.dialogs.openTagEditor(77);
      expect(tagEditorSong).toBe(77);

      await api.dialogs.closeAll();
      expect(shortcutsOpen).toBe(false);
      expect(tagEditorSong).toBeNull();
    });

    it("controls welcome and walkthrough states", () => {
      const api = createScriptingApi();

      api.dialogs.welcome.show();
      expect(welcomeStore.hasSeen).toBe(false);

      api.dialogs.welcome.dismiss();
      expect(welcomeStore.hasSeen).toBe(true);

      const startSpy = vi.spyOn(walkthroughStore, "start").mockImplementation(() => {});
      const stopSpy = vi.spyOn(walkthroughStore, "finish").mockImplementation(() => {});
      const nextSpy = vi.spyOn(walkthroughStore, "next").mockImplementation(() => {});

      api.dialogs.walkthrough.start();
      expect(startSpy).toHaveBeenCalledWith("full");

      api.dialogs.walkthrough.next();
      expect(nextSpy).toHaveBeenCalled();

      api.dialogs.walkthrough.stop();
      expect(stopSpy).toHaveBeenCalled();
    });
  });

  describe("Wait controller", () => {
    it("waits for state condition", async () => {
      const api = createScriptingApi();
      let condition = false;

      setTimeout(() => {
        condition = true;
      }, 50);

      await api.wait.forState(() => condition, 1000);
      expect(condition).toBe(true);
    });

    it("times out if condition is never satisfied", async () => {
      const api = createScriptingApi();
      await expect(api.wait.forState(() => false, 50)).rejects.toThrow("Timed out waiting for state condition");
    });
  });

  describe("View controller", () => {
    it("sets view and seekbar modes idempotently", async () => {
      const api = createScriptingApi();
      await api.view.setViewMode("albums", "rows");
      await api.view.setViewMode("albums", "rows");
      expect(prefs.albumsViewMode).toBe("rows");

      await api.view.setSeekbarMode("bands");
      await api.view.setSeekbarMode("bands");
      expect(prefs.seekBarMode).toBe("bands");
      await api.view.setSeekbarMode("waveform");
      expect(prefs.seekBarMode).toBe("waveform");
    });

    it("shows and hides a song-table column, naming valid columns for an unknown one", async () => {
      const api = createScriptingApi();
      await api.view.setColumnVisible("initial_key", true);
      expect(collectionStore.visibleColumns.initial_key).toBe(true);
      await api.view.setColumnVisible("initial_key", true);
      expect(collectionStore.visibleColumns.initial_key).toBe(true);
      await api.view.setColumnVisible("initial_key", false);
      expect(collectionStore.visibleColumns.initial_key).toBe(false);
      await expect(api.view.setColumnVisible("nope", true)).rejects.toThrow(/Unknown song-table column "nope"/);
    });

    it("opens the smart-playlist builder with the given rules", async () => {
      const api = createScriptingApi();
      await api.view.openSmartPlaylistBuilder([{ field: "year", op: ">=", value: "1980" }]);
      expect(collectionStore.isSmartBuilderOpen).toBe(true);
      expect(collectionStore.smartBuilderRules).toEqual([{ field: "year", op: ">=", value: "1980" }]);

      await api.dialogs.closeAll();
      expect(collectionStore.isSmartBuilderOpen).toBe(false);
    });

    it("navigates to the Playlists sub-tabs", async () => {
      const api = createScriptingApi();
      await api.navigate.to("playlists", "auto");
      expect(navigationStore.activeTab).toBe("playlists");
      expect(navigationStore.playlistsSubTab).toBe("auto");
    });
  });

  describe("Screens controller", () => {
    it("calls a registered screen hook and stops after it unregisters", async () => {
      const api = createScriptingApi();
      const seen: string[] = [];
      const off = registerScreenHook("search", async (q) => { seen.push(q); });

      await api.screens.search("evan");
      expect(seen).toEqual(["evan"]);

      off();
      await expect(api.screens.search("x")).rejects.toThrow(/needs the top navigation bar on screen/);
    });

    it("says which screen to open when its hook is missing", async () => {
      const api = createScriptingApi();
      await expect(api.screens.openAlbumEditor()).rejects.toThrow(/navigate\.album first/);
    });
  });
});
