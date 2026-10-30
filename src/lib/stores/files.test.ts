import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ConnectedDevice, FileSourceSummary } from "../types";

const handlers = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: (event: { payload: unknown }) => void) => {
    handlers.set(event, callback);
    return () => {};
  }),
}));

let sources: FileSourceSummary[] = [];
vi.mocked(invoke).mockImplementation(async (cmd: string) => {
  if (cmd === "list_file_sources") return sources;
  if (cmd === "get_connected_devices") return [];
  return null;
});

import { filesStore, sameSource } from "./files.svelte";
import { navigationStore } from "./navigation.svelte";

const music = { kind: "folder", id: 1 } as const;
const server = { kind: "subsonic", id: 2 } as const;
const player = { kind: "device", id: "E:\\" } as const;

function summary(source: FileSourceSummary["source"], name: string): FileSourceSummary {
  return { source, name, songCount: 3, available: true };
}

describe("filesStore", () => {
  beforeEach(async () => {
    await vi.waitFor(() => expect(filesStore.loaded).toBe(true));
    sources = [summary(music, "Music"), summary(server, "Navidrome")];
    await filesStore.refresh();
    handlers.get("devices-changed")!({ payload: [{ id: player.id, defaultName: "Walkman", profile: {} } satisfies ConnectedDevice] });
  });

  it("names and finds the listed sources and connected devices", () => {
    expect(filesStore.exists(music)).toBe(true);
    expect(filesStore.name(server)).toBe("Navidrome");
    expect(filesStore.exists(player)).toBe(true);
    expect(filesStore.exists({ kind: "webdav", id: 2 })).toBe(false);
    expect(filesStore.name({ kind: "folder", id: 9 })).toBe("");
  });

  it("tells sources apart by kind as well as id", () => {
    expect(sameSource({ kind: "folder", id: 2 }, server)).toBe(false);
    expect(sameSource({ kind: "subsonic", id: 2 }, server)).toBe(true);
  });

  it("leaves a folder whose server was removed for the Files overview", async () => {
    navigationStore.viewFiles(server, ["Artist"]);

    sources = [summary(music, "Music")];
    await filesStore.refresh();

    expect(navigationStore.activeTab).toBe("files");
    expect(navigationStore.filesLocation).toBeNull();
  });

  it("leaves a device that was unplugged, and keeps a source that is still there", () => {
    navigationStore.viewFiles(player, ["Music"]);
    handlers.get("devices-changed")!({ payload: [] });
    expect(navigationStore.filesLocation).toBeNull();

    navigationStore.viewFiles(music, ["Rock"]);
    handlers.get("devices-changed")!({ payload: [] });
    expect(navigationStore.filesLocation).toEqual({ source: music, path: ["Rock"] });
  });
});
