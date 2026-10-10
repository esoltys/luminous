import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { flushSync } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import InfoSidebarToggle from "./InfoSidebarToggle.svelte";
import { inspectorStore } from "../stores/inspector.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playerStore } from "../stores/player.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import type { Song } from "../types";

const song = (id: number) => ({ id, title: `Song ${id}`, artist: "Pink Floyd", album: "Dark Side" }) as Song;

describe("InfoSidebarToggle", () => {
  beforeEach(async () => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "get_songs_by_album" ? [song(100)] : null));
    inspectorStore.clearAll();
    playerStore.currentSong = song(1);
    windowLayoutStore.rightPanelOpen = true;
    navigationStore.activeTab = "collection";
    navigationStore.selectedArtistName = null;
    // Re-open the album so the inspector resolves it afresh after clearAll().
    navigationStore.selectedAlbumName = null;
    flushSync();
    navigationStore.selectedAlbumName = "Dark Side";
    await vi.waitFor(() => expect(inspectorStore.isShowingViewed).toBe(true));
  });

  it("is pressed while the sidebar shows this view's info, and a click hides the sidebar", async () => {
    const { getByRole } = render(InfoSidebarToggle, { props: { label: "Album Info" } });
    const pill = getByRole("button", { name: "Album Info" });
    expect(pill).toHaveAttribute("aria-pressed", "true");

    await fireEvent.click(pill);
    expect(windowLayoutStore.rightPanelOpen).toBe(false);
    expect(pill).toHaveAttribute("aria-pressed", "false");
  });

  it("brings the view's info back from Now Playing without closing the sidebar", async () => {
    const { getByRole } = render(InfoSidebarToggle, { props: { label: "Album Info" } });
    const pill = getByRole("button", { name: "Album Info" });

    inspectorStore.showPlaying();
    await vi.waitFor(() => expect(pill).toHaveAttribute("aria-pressed", "false"));

    await fireEvent.click(pill);
    expect(windowLayoutStore.rightPanelOpen).toBe(true);
    expect(pill).toHaveAttribute("aria-pressed", "true");
    expect(inspectorStore.subject).toMatchObject({ kind: "album", source: "view" });
  });

  it("opens a closed sidebar on this view's info", async () => {
    windowLayoutStore.rightPanelOpen = false;
    inspectorStore.showPlaying();
    const { getByRole } = render(InfoSidebarToggle, { props: { label: "Album Info" } });

    await fireEvent.click(getByRole("button", { name: "Album Info" }));
    expect(windowLayoutStore.rightPanelOpen).toBe(true);
    expect(inspectorStore.isShowingViewed).toBe(true);
  });

  it("is absent when the window is too narrow to show the sidebar", () => {
    const width = windowLayoutStore.viewportWidth;
    windowLayoutStore.viewportWidth = 500;
    try {
      const { queryByRole } = render(InfoSidebarToggle, { props: { label: "Album Info" } });
      expect(queryByRole("button", { name: "Album Info" })).not.toBeInTheDocument();
    } finally {
      windowLayoutStore.viewportWidth = width;
    }
  });
});
