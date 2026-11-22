import "@testing-library/jest-dom";
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { flushSync } from "svelte";
import StationListImportDialog from "./StationListImportDialog.svelte";
import { playerStore } from "../../stores/player.svelte";
import { radioStore } from "../../stores/radio.svelte";

const lists = [
  { path: "G:\\Playlists\\soma_fm.m3u", name: "soma_fm", station_count: 43 },
  { path: "/home/me/jazz.pls", name: "jazz", station_count: 2 },
];

describe("StationListImportDialog (#2279)", () => {
  afterEach(() => {
    playerStore.pendingStationLists = [];
    vi.restoreAllMocks();
  });

  it("stays hidden with nothing to offer", () => {
    render(StationListImportDialog);
    expect(screen.queryByText("Import radio stations?")).toBeNull();
  });

  it("totals the stations, names the files, and imports them on confirm", async () => {
    const importLists = vi.spyOn(radioStore, "importStationLists").mockResolvedValue();
    playerStore.pendingStationLists = lists;
    render(StationListImportDialog);

    expect(
      screen.getByText(/Found 45 streaming stations in soma_fm\.m3u, jazz\.pls/),
    ).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "Import" }));
    flushSync();

    expect(importLists).toHaveBeenCalledWith(lists);
    expect(playerStore.pendingStationLists).toEqual([]);
    expect(screen.queryByText("Import radio stations?")).toBeNull();
  });

  it("drops the offer on cancel without importing", async () => {
    const importLists = vi.spyOn(radioStore, "importStationLists").mockResolvedValue();
    playerStore.pendingStationLists = lists;
    render(StationListImportDialog);

    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    flushSync();

    expect(importLists).not.toHaveBeenCalled();
    expect(playerStore.pendingStationLists).toEqual([]);
  });
});
