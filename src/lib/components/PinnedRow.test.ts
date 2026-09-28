import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import PinnedRow from "./PinnedRow.svelte";
import { pinnedStore } from "../stores/pinned.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import type { PinnedItem, Song, AlbumItem, ArtistItem, Playlist, AutoPlaylistItem } from "../types";

describe("PinnedRow.svelte", () => {
  const mockSong: Song = {
    id: 101,
    source: "local_file",
    filetype: "FLAC",
    title: "Paranoid Android",
    artist: "Radiohead",
    album: "OK Computer",
    art_embedded: false,
    art_unset: false,
    compilation: false,
    beginning_nanosec: 0,
    end_nanosec: 0,
    rating: 5,
    playcount: 42,
    skipcount: 1,
    length_nanosec: 387_000_000_000,
    added: 1_700_000_000,
    year: 1997,
    unavailable: false,
  };

  const mockAlbum: AlbumItem = {
    album: "OK Computer",
    artist: "Radiohead",
    year: 1997,
    track_count: 12,
    disc_count: 1,
    art_embedded: false,
    art_automatic: null,
    art_manual: null,
    genre: "Alternative",
    rating: 5,
    total_duration_nanosec: 0,
  };

  const mockArtist: ArtistItem = {
    name: "Portishead",
    album_count: 3,
    song_count: 30,
    genre: "Trip Hop",
  };

  const mockPlaylist: Playlist = {
    id: 42,
    name: "My Best Songs",
    dynamic_enabled: false,
    is_queue: false,
    created: 0,
    updated: 0,
    track_count: 15,
  };

  const mockAutoPlaylist: AutoPlaylistItem = {
    kind: "favourites",
    trackCount: 20,
  };

  const allItems: PinnedItem[] = [
    { type: "album", album: mockAlbum },
    { type: "song", song: mockSong },
    { type: "artist", artist: mockArtist },
    { type: "playlist", playlist: mockPlaylist },
    { type: "auto_playlist", autoPlaylist: mockAutoPlaylist },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    pinnedStore.items = [...allItems];
  });

  it("renders pinned cards when items exist", () => {
    const { getByText } = render(PinnedRow);

    expect(getByText("OK Computer")).toBeInTheDocument();
    expect(getByText("Paranoid Android")).toBeInTheDocument();
    expect(getByText("Portishead")).toBeInTheDocument();
    expect(getByText("My Best Songs")).toBeInTheDocument();
    expect(getByText("Favourite Songs")).toBeInTheDocument();
  });

  it("opens AlbumContextMenu on right-click on album card", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const { getByText } = render(PinnedRow);

    const albumEl = getByText("OK Computer");
    await fireEvent.contextMenu(albumEl);

    expect(await screen.findByText("Play Album")).toBeInTheDocument();
    expect(await screen.findByText("Unpin from Home")).toBeInTheDocument();
  });

  it("opens SongContextMenu on right-click on song card", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const { getByText } = render(PinnedRow);

    const songEl = getByText("Paranoid Android");
    await fireEvent.contextMenu(songEl);

    expect(await screen.findByText("Play Song")).toBeInTheDocument();
    expect(await screen.findByText("Unpin from Home")).toBeInTheDocument();
  });

  it("opens ArtistContextMenu on right-click on artist card", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const { getByText } = render(PinnedRow);

    const artistEl = getByText("Portishead");
    await fireEvent.contextMenu(artistEl);

    expect(await screen.findByText("Play Artist")).toBeInTheDocument();
    expect(await screen.findByText("Unpin from Home")).toBeInTheDocument();
  });

  it("opens PlaylistCardContextMenu on right-click on playlist card", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const { getByText } = render(PinnedRow);

    const playlistEl = getByText("My Best Songs");
    await fireEvent.contextMenu(playlistEl);

    expect(await screen.findByText("Play Playlist")).toBeInTheDocument();
    expect(await screen.findByText("Unpin from Home")).toBeInTheDocument();
  });

  it("opens AutoPlaylistContextMenu on right-click on auto-playlist card", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const { getByText } = render(PinnedRow);

    const autoPlEl = getByText("Favourite Songs");
    await fireEvent.contextMenu(autoPlEl);

    expect(await screen.findByText("Play Playlist")).toBeInTheDocument();
    expect(await screen.findByText("Unpin from Home")).toBeInTheDocument();
  });

  it("unpins item from Home when clicking Unpin from Home in context menu", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const toggleSpy = vi.spyOn(pinnedStore, "toggle").mockResolvedValue();
    const { getByText } = render(PinnedRow);

    const albumEl = getByText("OK Computer");
    await fireEvent.contextMenu(albumEl);

    const unpinBtn = await screen.findByText("Unpin from Home");
    await fireEvent.click(unpinBtn);

    expect(toggleSpy).toHaveBeenCalledWith("album", "OK Computer");
  });

  it("hides pinned playlists and auto-playlists that have no songs", () => {
    pinnedStore.items = [
      { type: "album", album: mockAlbum },
      { type: "playlist", playlist: { ...mockPlaylist, track_count: 0 } },
      { type: "auto_playlist", autoPlaylist: { kind: "favourites", trackCount: 0 } },
    ];
    const { getByText, queryByText } = render(PinnedRow);

    expect(getByText("OK Computer")).toBeInTheDocument();
    expect(queryByText("My Best Songs")).not.toBeInTheDocument();
    expect(queryByText("Favourite Songs")).not.toBeInTheDocument();
  });

  it("hides the whole row when every pin is an empty playlist", () => {
    pinnedStore.items = [
      { type: "auto_playlist", autoPlaylist: { kind: "daypart", trackCount: 0 } },
      { type: "auto_playlist", autoPlaylist: { kind: "favourites", trackCount: 0 } },
    ];
    const { queryByText } = render(PinnedRow);

    expect(queryByText("Pinned")).not.toBeInTheDocument();
  });

  it("navigates to and selects custom playlist when clicking a pinned playlist card", async () => {
    playlistsStore.playlists = [
      { id: 1, name: "Queue", is_queue: true, track_count: 0 } as Playlist,
      { id: 42, name: "My Best Songs", is_queue: false, track_count: 15 } as Playlist,
    ];
    playlistsStore.activePlaylistId = 1;
    navigationStore.activeTab = "home";

    const { getByText } = render(PinnedRow);
    const playlistCard = getByText("My Best Songs");
    await fireEvent.click(playlistCard);

    expect(navigationStore.activeTab).toBe("playlists");
    expect(navigationStore.selectedPlaylistId).toBe(42);
    expect(playlistsStore.activePlaylistId).toBe(42);
  });
});
