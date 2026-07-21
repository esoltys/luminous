import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Sidebar from "./Sidebar.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { playlistsStore } from "../stores/playlists.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("Sidebar.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.activeTab = "collection";
    collectionStore.activeSubTab = "songs";
    collectionStore.stats = {
      total_songs: 10,
      total_albums: 5,
      total_artists: 2,
      total_duration_nanosec: 1000,
      total_filesize_bytes: 1000,
    };
  });

  it("expands Collection sub-items (Artists, Albums, Songs) when Collection tab is active", () => {
    const { getByRole } = render(Sidebar, { props: { width: 256 } });
    expect(getByRole("button", { name: /artists/i })).toBeInTheDocument();
    expect(getByRole("button", { name: /albums/i })).toBeInTheDocument();
    expect(getByRole("button", { name: /songs/i })).toBeInTheDocument();
  });

  it("switches activeSubTab when clicking sub-items in Sidebar", async () => {
    const { getByRole } = render(Sidebar, { props: { width: 256 } });

    const artistsBtn = getByRole("button", { name: /artists/i });
    await fireEvent.click(artistsBtn);
    expect(collectionStore.activeSubTab).toBe("artists");

    const albumsBtn = getByRole("button", { name: /albums/i });
    await fireEvent.click(albumsBtn);
    expect(collectionStore.activeSubTab).toBe("albums");

    const songsBtn = getByRole("button", { name: /songs/i });
    await fireEvent.click(songsBtn);
    expect(collectionStore.activeSubTab).toBe("songs");
  });

  it("hides Collection sub-items when a non-collection tab is active", () => {
    collectionStore.activeTab = "home";
    const { queryByRole } = render(Sidebar, { props: { width: 256 } });
    expect(queryByRole("button", { name: /artists/i })).toBeNull();
    expect(queryByRole("button", { name: /albums/i })).toBeNull();
  });

  it("centers Collection and Playlists wrapper containers when collapsed (width < 180)", () => {
    const { getByTitle } = render(Sidebar, { props: { width: 64 } });
    const collectionBtn = getByTitle("Collection");
    const playlistsBtn = getByTitle("Playlists");
    expect(collectionBtn.parentElement).toHaveClass("items-center");
    expect(playlistsBtn.parentElement).toHaveClass("items-center");
  });
});
