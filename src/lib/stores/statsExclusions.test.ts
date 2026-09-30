import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { statsExclusionsStore, toggleStatsExcluded } from "./statsExclusions.svelte";
import { toastStore } from "./toast.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("StatsExclusionsStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const m of [...toastStore.messages]) toastStore.dismiss(m.id);
  });

  it("loads exclusions on refresh and reports isExcluded", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_stats_exclusions") {
        return [
          ["album", "Abbey Road"],
          ["artist", "The Beatles"],
          ["song", "42"],
          ["genre", "Rock"],
        ];
      }
      return undefined;
    });

    await statsExclusionsStore.refresh();

    expect(statsExclusionsStore.isExcluded("album", "Abbey Road")).toBe(true);
    expect(statsExclusionsStore.isExcluded("artist", "The Beatles")).toBe(true);
    expect(statsExclusionsStore.isExcluded("song", "42")).toBe(true);
    expect(statsExclusionsStore.isExcluded("genre", "Rock")).toBe(true);
    expect(statsExclusionsStore.isExcluded("album", "Revolver")).toBe(false);
  });

  it("sets exclusion status and refreshes", async () => {
    let exclusions: [string, string][] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        if (a.excluded) {
          exclusions = [[a.entityType, a.entityKey]];
        } else {
          exclusions = [];
        }
      }
      if (cmd === "get_stats_exclusions") return exclusions;
      return undefined;
    });

    await statsExclusionsStore.setExcluded("album", "Abbey Road", true);
    expect(invoke).toHaveBeenCalledWith("set_stats_excluded", {
      entityType: "album",
      entityKey: "Abbey Road",
      excluded: true,
    });
    expect(statsExclusionsStore.isExcluded("album", "Abbey Road")).toBe(true);

    await statsExclusionsStore.setExcluded("album", "Abbey Road", false);
    expect(statsExclusionsStore.isExcluded("album", "Abbey Road")).toBe(false);
  });

  it("toggles exclusion status", async () => {
    let exclusions: [string, string][] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        if (a.excluded) {
          exclusions.push([a.entityType, a.entityKey]);
        } else {
          exclusions = exclusions.filter(([t, k]) => !(t === a.entityType && k === a.entityKey));
        }
      }
      if (cmd === "get_stats_exclusions") return exclusions;
      return undefined;
    });
    await statsExclusionsStore.refresh();

    await statsExclusionsStore.toggle("album", "Abbey Road");
    expect(statsExclusionsStore.isExcluded("album", "Abbey Road")).toBe(true);

    await statsExclusionsStore.toggle("album", "Abbey Road");
    expect(statsExclusionsStore.isExcluded("album", "Abbey Road")).toBe(false);
  });

  it("toggleWithToast excludes an item and shows excluded toast", async () => {
    let exclusions: [string, string][] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        if (a.excluded) exclusions = [[a.entityType, a.entityKey]];
        else exclusions = [];
      }
      if (cmd === "get_stats_exclusions") return exclusions;
      return undefined;
    });
    await statsExclusionsStore.refresh();

    const toastSpy = vi.spyOn(toastStore, "show");

    await statsExclusionsStore.toggleWithToast("album", "Abbey Road");
    expect(statsExclusionsStore.isExcluded("album", "Abbey Road")).toBe(true);
    expect(toastSpy).toHaveBeenCalledWith("Excluded Abbey Road from Stats");

    await statsExclusionsStore.toggleWithToast("album", "Abbey Road");
    expect(statsExclusionsStore.isExcluded("album", "Abbey Road")).toBe(false);
    expect(toastSpy).toHaveBeenCalledWith("Included Abbey Road in Stats again");
  });

  it("toggleWithToast uses fallback name when album name is empty", async () => {
    let exclusions: [string, string][] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        if (a.excluded) exclusions = [[a.entityType, a.entityKey]];
        else exclusions = [];
      }
      if (cmd === "get_stats_exclusions") return exclusions;
      return undefined;
    });
    await statsExclusionsStore.refresh();

    const toastSpy = vi.spyOn(toastStore, "show");

    await statsExclusionsStore.toggleWithToast("album", "");
    expect(toastSpy).toHaveBeenCalledWith("Excluded Unknown Album from Stats");
  });

  it("toggleStatsExcluded helper invokes toggleWithToast", async () => {
    const toggleSpy = vi.spyOn(statsExclusionsStore, "toggleWithToast").mockResolvedValue(undefined);

    await toggleStatsExcluded("artist", "Radiohead");
    expect(toggleSpy).toHaveBeenCalledWith("artist", "Radiohead", undefined);
  });
});
