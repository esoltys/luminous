import "@testing-library/jest-dom";
import { tick } from "svelte";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Sidebar from "./Sidebar.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { pinnedStore } from "../stores/pinned.svelte";
import { i18n } from "../stores/i18n.svelte";
import { musicbrainzStore } from "../stores/musicbrainz.svelte";
import { scrobblerStore } from "../stores/scrobbler.svelte";
import { filesStore } from "../stores/files.svelte";

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
    const { getByText, queryByText, getAllByTitle, rerender } = render(Sidebar, {
      props: { width: 256 },
    });
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

  describe("Integrations connection indicator", () => {
    beforeEach(() => {
      musicbrainzStore.isLoggedIn = true;
      musicbrainzStore.username = "soltys";
      scrobblerStore.enabled = true;
      scrobblerStore.paused = false;
    });

    afterEach(() => {
      musicbrainzStore.isLoggedIn = false;
      musicbrainzStore.username = null;
      scrobblerStore.enabled = false;
      scrobblerStore.paused = false;
    });

    it("shows a filled dot and Connected title while scrobbling is active", () => {
      const { getByTestId, getByTitle } = render(Sidebar, { props: { width: 256 } });
      expect(getByTestId("musicbrainz-status-dot")).toHaveClass("bg-brand-text-primary");
      expect(getByTitle("Connected")).toBeInTheDocument();
    });

    it("shows a hollow dot, title suffix and label while scrobbling is paused", () => {
      scrobblerStore.paused = true;
      const { getByTestId, getByTitle } = render(Sidebar, { props: { width: 256 } });
      const dot = getByTestId("musicbrainz-status-dot");
      expect(dot).toHaveClass("bg-transparent");
      expect(dot).toHaveAttribute("aria-label", "Scrobbling paused");
      expect(getByTitle("Connected – Scrobbling paused")).toBeInTheDocument();
    });

    it("ignores a stale paused flag when scrobbling is disabled", () => {
      scrobblerStore.enabled = false;
      scrobblerStore.paused = true;
      const { getByTestId } = render(Sidebar, { props: { width: 256 } });
      expect(getByTestId("musicbrainz-status-dot")).toHaveClass("bg-brand-text-primary");
    });

    it("shows Connect... when not connected and opens integrations settings on click", async () => {
      musicbrainzStore.isLoggedIn = false;
      musicbrainzStore.username = null;
      scrobblerStore.enabled = false;
      scrobblerStore.lastfmEnabled = false;
      scrobblerStore.lastfmUsername = null;
      scrobblerStore.username = null;

      const openSettingsSpy = vi.spyOn(navigationStore, "openSettings");
      const { getByTitle } = render(Sidebar, { props: { width: 256 } });
      const connectBtn = getByTitle("Connect...");
      expect(connectBtn).toBeInTheDocument();

      await fireEvent.click(connectBtn);
      expect(openSettingsSpy).toHaveBeenCalledWith("integrations");
    });
  });
});

describe("Sidebar.svelte - Files section", () => {
  const music = { kind: "folder", id: 1 } as const;

  beforeEach(() => {
    i18n.currentLocale = "en-CA";
    navigationStore.activeTab = "collection";
    navigationStore.filesLocation = null;
    filesStore.loaded = true;
    filesStore.sources = [];
  });

  it("offers to add a music folder while the library is empty", () => {
    collectionStore.stats = { ...collectionStore.stats!, total_songs: 0 };
    const { getByRole, queryByRole } = render(Sidebar, { props: { width: 256 } });

    expect(getByRole("button", { name: "Files" })).toBeInTheDocument();
    expect(getByRole("button", { name: /add a music folder/i })).toBeInTheDocument();
    expect(queryByRole("button", { name: /artists/i })).toBeNull();
  });

  it("lists each source with its song count and opens it in the Files view", async () => {
    filesStore.sources = [{ source: music, name: "Music", songCount: 1234, available: true }];
    const { getByRole, queryByRole } = render(Sidebar, { props: { width: 256 } });

    expect(queryByRole("button", { name: /add a music folder/i })).toBeNull();
    await fireEvent.click(getByRole("button", { name: /Music \(1,234\)/ }));
    expect(navigationStore.activeTab).toBe("files");
    expect(navigationStore.filesLocation).toEqual({ source: music, path: [] });

    await fireEvent.click(getByRole("button", { name: "Files" }));
    expect(navigationStore.filesLocation).toBeNull();
  });
});

describe("Sidebar.svelte - File Drag & Drop (#1888)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
    navigationStore.activeTab = "playlists";
    collectionStore.statsLoaded = true;
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
    playlistsStore.playlists = [
      { id: 1, name: "Queue", is_queue: true } as any,
      { id: 2, name: "Favorites", is_queue: false } as any,
    ];
  });

  it("exposes Queue drop target and highlights during drag", async () => {
    const { songDrag, songsPayload } = await import("../stores/songDrag.svelte");
    songDrag.start(songsPayload([{ id: 10, title: "Track" } as any]), { x: 0, y: 0 });

    const { getByRole } = render(Sidebar, { props: { width: 256 } });
    const queueBtn = getByRole("button", { name: /queue/i });
    expect(queueBtn.getAttribute("data-drop-target")).toBe("queue");

    songDrag.dropTarget = { kind: "queue" };
    await tick();
    expect(queueBtn.className).toContain("ring-brand-accent");

    songDrag.end();
  });

  it("expands custom playlists after dragging over Custom for 500ms", async () => {
    vi.useFakeTimers();
    try {
      const { songDrag, songsPayload } = await import("../stores/songDrag.svelte");
      songDrag.start(songsPayload([{ id: 10, title: "Track" } as any]), { x: 0, y: 0 });

      const { getByRole, findByRole } = render(Sidebar, { props: { width: 256 } });
      const customBtn = getByRole("button", { name: /custom/i });
      expect(customBtn.getAttribute("data-drop-target")).toBe("custom-category");

      songDrag.dropTarget = { kind: "custom-category" };
      await tick();
      vi.advanceTimersByTime(500);
      await tick();

      const favPlaylistBtn = await findByRole("button", { name: "Favorites" });
      expect(favPlaylistBtn).toBeInTheDocument();
      expect(favPlaylistBtn.getAttribute("data-drop-target")).toBe("playlist");
      expect(favPlaylistBtn.getAttribute("data-playlist-id")).toBe("2");

      songDrag.end();
    } finally {
      vi.useRealTimers();
    }
  });

  it("offers only hand-built playlists as drop targets, not smart ones", async () => {
    // The smart playlist still counts towards Custom and opens from it; it just can't take a drop,
    // because its rules decide what it holds.
    playlistsStore.playlists = [
      ...playlistsStore.playlists,
      {
        id: 3,
        name: "Rock Rules",
        is_queue: false,
        dynamic_enabled: true,
        dynamic_spec: "genre:rock",
      } as any,
    ];
    vi.useFakeTimers();
    try {
      const { songDrag, songsPayload } = await import("../stores/songDrag.svelte");
      songDrag.start(songsPayload([{ id: 10, title: "Track" } as any]), { x: 0, y: 0 });
      const { findByRole, queryByRole } = render(Sidebar, { props: { width: 256 } });

      songDrag.dropTarget = { kind: "custom-category" };
      await tick();
      vi.advanceTimersByTime(500);
      await tick();

      expect(await findByRole("button", { name: "Favorites" })).toHaveAttribute(
        "data-drop-target",
        "playlist",
      );
      expect(queryByRole("button", { name: "Rock Rules" })).toBeNull();
      songDrag.end();
    } finally {
      vi.useRealTimers();
    }
  });

  it("doesn't offer Custom as a target when it holds only smart playlists", () => {
    playlistsStore.playlists = [
      { id: 1, name: "Queue", is_queue: true } as any,
      {
        id: 3,
        name: "Rock Rules",
        is_queue: false,
        dynamic_enabled: true,
        dynamic_spec: "genre:rock",
      } as any,
    ];
    const { getByRole } = render(Sidebar, { props: { width: 256 } });
    expect(getByRole("button", { name: /custom/i })).not.toHaveAttribute("data-drop-target");
  });
});
