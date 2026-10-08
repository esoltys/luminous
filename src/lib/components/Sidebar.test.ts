import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Sidebar from "./Sidebar.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { pinnedStore } from "../stores/pinned.svelte";
import { i18n } from "../stores/i18n.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("Sidebar.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
    navigationStore.activeTab = "collection";
    navigationStore.activeSubTab = "songs";
    collectionStore.stats = {
      total_songs: 10,
      total_albums: 5,
      total_artists: 2,
      total_duration_nanosec: 1000,
      total_filesize_bytes: 1000,
      album_art_bytes: 0,
      artist_art_bytes: 0,
      thumbnail_bytes: 0,
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
    expect(navigationStore.activeSubTab).toBe("artists");

    const albumsBtn = getByRole("button", { name: /albums/i });
    await fireEvent.click(albumsBtn);
    expect(navigationStore.activeSubTab).toBe("albums");

    const songsBtn = getByRole("button", { name: /songs/i });
    await fireEvent.click(songsBtn);
    expect(navigationStore.activeSubTab).toBe("songs");
  });

  it("keeps Collection sub-items expanded even when a non-collection tab is active", () => {
    navigationStore.activeTab = "home";
    const { queryByRole } = render(Sidebar, { props: { width: 256 } });
    expect(queryByRole("button", { name: /artists/i })).not.toBeNull();
    expect(queryByRole("button", { name: /albums/i })).not.toBeNull();
  });

  it("keeps Collection sub-items visible after clicking the Collection header repeatedly", async () => {
    const { getByTitle, queryByRole } = render(Sidebar, { props: { width: 256 } });
    const collectionBtn = getByTitle("Collection");

    await fireEvent.click(collectionBtn);
    expect(queryByRole("button", { name: /artists/i })).not.toBeNull();

    await fireEvent.click(collectionBtn);
    expect(queryByRole("button", { name: /artists/i })).not.toBeNull();
  });

  it("centers Collection and Playlists wrapper containers when collapsed (width < 180)", () => {
    const { getByTitle } = render(Sidebar, { props: { width: 64 } });
    const collectionBtn = getByTitle("Collection");
    const playlistsBtn = getByTitle("Playlists");
    expect(collectionBtn.parentElement).toHaveClass("items-center");
    expect(playlistsBtn.parentElement).toHaveClass("items-center");
  });

  it("animates width changes by default but suppresses the transition while actively resizing", () => {
    const { container, rerender } = render(Sidebar, { props: { width: 256, resizing: false } });
    const aside = container.querySelector("aside");
    expect(aside).toHaveClass("transition-[width]");
    expect(aside).not.toHaveClass("transition-none");

    rerender({ width: 200, resizing: true });
    expect(aside).toHaveClass("transition-none");
  });

  it("formats collection numbers according to the active locale", () => {
    i18n.currentLocale = "fr-CA";
    collectionStore.stats = {
      ...collectionStore.stats,
      total_songs: 3095,
    };
    const { getByRole } = render(Sidebar, { props: { width: 256 } });
    const songsBtn = getByRole("button", { name: /chansons|songs/i });
    expect(songsBtn.textContent).toMatch(/3[\s\u202f]095/);
  });

  it("renders pinned navigation items in expanded and collapsed sidebar modes", async () => {
    pinnedStore.items = [
      {
        type: "album",
        album: {
          album: "OK Computer",
          artist: "Radiohead",
          rating: 5,
        } as any,
      },
    ];

    // Expanded sidebar
    const { getByText, queryByText, getAllByTitle, rerender } = render(Sidebar, { props: { width: 256 } });
    expect(getByText("OK Computer")).toBeInTheDocument();
    expect(getByText("Radiohead")).toBeInTheDocument();

    // Collapsed sidebar
    rerender({ width: 64 });
    await vi.waitFor(() => {
      expect(queryByText("Radiohead")).not.toBeInTheDocument();
    });
    const pinnedBtn = getAllByTitle("OK Computer • Radiohead");
    expect(pinnedBtn.length).toBeGreaterThan(0);
  });
});

