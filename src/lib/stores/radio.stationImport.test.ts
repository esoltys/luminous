import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { radioStore } from "./radio.svelte";
import { toastStore } from "./toast.svelte";
import { navigationStore } from "./navigation.svelte";
import type { StationListFile } from "../types";

const somaFm: StationListFile = {
  path: "G:/Playlists/soma_fm.m3u",
  name: "soma_fm",
  station_count: 43,
};
const groove: StationListFile = {
  path: "G:/Playlists/groove.pls",
  name: "groove",
  station_count: 2,
};

describe("radioStore.importStationLists (#2279)", () => {
  const defaultInvoke = vi.mocked(invoke).getMockImplementation()!;
  let failPath: string | null;

  beforeEach(() => {
    failPath = null;
    toastStore.messages = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "import_playlist") {
        if (args.filePath === failPath) throw new Error("disk gone");
        const name = args.filePath === somaFm.path ? "soma_fm" : "groove";
        return { id: 900, name };
      }
      return defaultInvoke(cmd, args);
    });
  });

  afterEach(() => {
    vi.mocked(invoke).mockImplementation(defaultInvoke);
  });

  it("imports each file as a playlist and refreshes the lists My Stations reads", async () => {
    await radioStore.importStationLists([somaFm, groove]);

    expect(invoke).toHaveBeenCalledWith("import_playlist", { filePath: somaFm.path });
    expect(invoke).toHaveBeenCalledWith("import_playlist", { filePath: groove.path });
    expect(invoke).toHaveBeenCalledWith("get_playlists");
    expect(invoke).toHaveBeenCalledWith("get_radio_playlist_ids");
    expect(toastStore.messages.map((m) => m.text)).toEqual([
      "Imported soma_fm with 43 stations",
      "Imported groove with 2 stations",
    ]);
  });

  it("the toast's Show action opens Internet Radio on My Stations", async () => {
    radioStore.subView = "discover";
    navigationStore.activeTab = "collection";

    await radioStore.importStationLists([somaFm]);
    toastStore.messages[0].action!.onClick();

    expect(navigationStore.activeTab).toBe("radio");
    expect(radioStore.subView).toBe("my_stations");
  });

  it("reports a file that fails to import and still imports the rest", async () => {
    failPath = somaFm.path;

    await radioStore.importStationLists([somaFm, groove]);

    const toasts = toastStore.messages.map((m) => [m.variant, m.text]);
    expect(toasts).toEqual([
      ["error", "Couldn't import soma_fm: disk gone"],
      ["success", "Imported groove with 2 stations"],
    ]);
  });

  it("does nothing for an empty list", async () => {
    vi.mocked(invoke).mockClear();
    await radioStore.importStationLists([]);
    expect(invoke).not.toHaveBeenCalled();
  });
});
