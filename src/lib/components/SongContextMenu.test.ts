import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SongContextMenu from "./SongContextMenu.svelte";
import { picardStore } from "../stores/picard.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { Song, Playlist } from "../types";

describe("SongContextMenu.svelte", () => {
  const baseSong: Omit<Song, "id" | "source"> = {
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

  const localSong: Song = { ...baseSong, id: 1, source: "local_file" };
  const webdavSong: Song = { ...baseSong, id: 2, source: "web_dav" };

  beforeEach(() => {
    picardStore.path = "/usr/bin/picard";
  });

  it("enables Open in Picard for a local song", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).not.toBeDisabled();
  });

  it("disables Open in Picard for a single WebDAV song, with a tooltip", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: webdavSong,
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    const button = item.closest("button");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Open in Picard isn't available for songs on a remote server");
  });

  it("disables Open in Picard for a song synced from an OpenSubsonic server", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: { ...baseSong, id: 3, source: "subsonic" },
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).toBeDisabled();
  });

  it("disables Open in Picard when every selected song is WebDAV", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: webdavSong,
      selectedCount: 2,
      selectedSongIds: [2, 3],
      selectedSongs: [webdavSong, { ...webdavSong, id: 3 }],
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).toBeDisabled();
  });

  it("keeps Open in Picard enabled for a mixed local + WebDAV selection", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      selectedCount: 2,
      selectedSongIds: [1, 2],
      selectedSongs: [localSong, webdavSong],
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).not.toBeDisabled();
  });

  it("disables Open in Picard when Picard itself isn't found, regardless of source", async () => {
    picardStore.path = null;
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    const button = item.closest("button");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "MusicBrainz Picard not found. Install it from picard.musicbrainz.org, or set a custom path in Settings."
    );
  });

  it("renders Add to Playlist and opens submenu listing custom playlists", async () => {
    playlistsStore.playlists = [
      { id: 10, name: "Queue", track_count: 0, created: 0, updated: 0, dynamic_enabled: false, is_queue: true },
      { id: 20, name: "Indie Pop", track_count: 5, created: 0, updated: 0, dynamic_enabled: false, is_queue: false },
      { id: 30, name: "Heavy Metal", track_count: 12, created: 0, updated: 0, dynamic_enabled: false, is_queue: false },
    ];
    playlistsStore.pinnedPlaylistId = null;

    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onClose: () => {},
    });

    const addToPlaylistBtn = await screen.findByText("Add to Playlist");
    expect(addToPlaylistBtn).toBeInTheDocument();

    await fireEvent.click(addToPlaylistBtn);

    expect(await screen.findByText("Indie Pop")).toBeInTheDocument();
    expect(await screen.findByText("Heavy Metal")).toBeInTheDocument();
    expect(await screen.findByText("New Playlist...")).toBeInTheDocument();
  });

  it("adds song directly to a non-active custom playlist", async () => {
    const addSongsSpy = vi.spyOn(playlistsStore, "addSongsToPlaylist").mockResolvedValue();
    const onClose = vi.fn();

    playlistsStore.playlists = [
      { id: 10, name: "Queue", track_count: 0, created: 0, updated: 0, dynamic_enabled: false, is_queue: true },
      { id: 20, name: "Active Playlist", track_count: 5, created: 0, updated: 0, dynamic_enabled: false, is_queue: false },
      { id: 30, name: "Target Playlist", track_count: 12, created: 0, updated: 0, dynamic_enabled: false, is_queue: false },
    ];
    playlistsStore.pinnedPlaylistId = 20;

    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onClose,
    });

    const addToPlaylistBtn = await screen.findByText("Add to Playlist");
    await fireEvent.click(addToPlaylistBtn);

    const targetPlBtn = await screen.findByText("Target Playlist");
    await fireEvent.click(targetPlBtn);

    expect(addSongsSpy).toHaveBeenCalledWith(30, [1]);
    expect(onClose).toHaveBeenCalled();
  });

  it("marks active custom playlist with an Active badge", async () => {
    playlistsStore.playlists = [
      { id: 10, name: "Queue", track_count: 0, created: 0, updated: 0, dynamic_enabled: false, is_queue: true },
      { id: 20, name: "Road Trip", track_count: 5, created: 0, updated: 0, dynamic_enabled: false, is_queue: false },
    ];
    playlistsStore.pinnedPlaylistId = 20;

    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onClose: () => {},
    });

    const addToPlaylistBtn = await screen.findByText("Add to Playlist");
    await fireEvent.click(addToPlaylistBtn);

    expect(await screen.findByText("Active")).toBeInTheDocument();
  });

  it("displays No custom playlists when user has no custom playlists", async () => {
    playlistsStore.playlists = [
      { id: 10, name: "Queue", track_count: 0, created: 0, updated: 0, dynamic_enabled: false, is_queue: true },
      { id: 20, name: "Rock Genre", track_count: 5, created: 0, updated: 0, dynamic_enabled: true, is_queue: false },
    ];
    playlistsStore.pinnedPlaylistId = null;

    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onClose: () => {},
    });

    const addToPlaylistBtn = await screen.findByText("Add to Playlist");
    await fireEvent.click(addToPlaylistBtn);

    expect(await screen.findByText("No custom playlists")).toBeInTheDocument();
    expect(await screen.findByText("New Playlist...")).toBeInTheDocument();
  });

  it("adds multiple selected songs directly to a custom playlist", async () => {
    const addSongsSpy = vi.spyOn(playlistsStore, "addSongsToPlaylist").mockResolvedValue();
    const onClose = vi.fn();

    playlistsStore.playlists = [
      { id: 10, name: "Queue", track_count: 0, created: 0, updated: 0, dynamic_enabled: false, is_queue: true },
      { id: 20, name: "Favorites", track_count: 2, created: 0, updated: 0, dynamic_enabled: false, is_queue: false },
    ];
    playlistsStore.pinnedPlaylistId = null;

    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      selectedCount: 3,
      selectedSongIds: [1, 2, 3],
      onPlay: () => {},
      onClose,
    });

    const addToPlaylistBtn = await screen.findByText("Add to Playlist");
    await fireEvent.click(addToPlaylistBtn);

    const favPlBtn = await screen.findByText("Favorites");
    await fireEvent.click(favPlBtn);

    expect(addSongsSpy).toHaveBeenCalledWith(20, [1, 2, 3]);
    expect(onClose).toHaveBeenCalled();
  });

  it("creates new playlist and adds song via New Playlist modal", async () => {
    const createPlaylistSpy = vi.spyOn(playlistsStore, "createPlaylist").mockResolvedValue({
      id: 99,
      name: "Chill Sunset",
      track_count: 0,
      created: 0,
      updated: 0,
      dynamic_enabled: false,
      is_queue: false,
    });
    const addSongsSpy = vi.spyOn(playlistsStore, "addSongsToPlaylist").mockResolvedValue();
    const onClose = vi.fn();

    playlistsStore.playlists = [
      { id: 10, name: "Queue", track_count: 0, created: 0, updated: 0, dynamic_enabled: false, is_queue: true },
    ];
    playlistsStore.pinnedPlaylistId = null;

    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onClose,
    });

    const addToPlaylistBtn = await screen.findByText("Add to Playlist");
    await fireEvent.click(addToPlaylistBtn);

    const newPlBtn = await screen.findByText("New Playlist...");
    await fireEvent.click(newPlBtn);

    const input = await screen.findByLabelText("Enter a name for the new playlist:");
    await fireEvent.input(input, { target: { value: "Chill Sunset" } });

    const createBtn = await screen.findByRole("button", { name: "Create" });
    await fireEvent.click(createBtn);

    expect(createPlaylistSpy).toHaveBeenCalledWith("Chill Sunset");
    expect(addSongsSpy).toHaveBeenCalledWith(99, [1]);
    expect(onClose).toHaveBeenCalled();
  });

  it("displays 'Include in Stats' and un-excludes album when song belongs to excluded album", async () => {
    let exclusions: [string, string][] = [["album", "OK Computer"]];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        if (a.excluded) exclusions = [[a.entityType, a.entityKey]];
        else exclusions = [];
      }
      if (cmd === "get_stats_exclusions") return exclusions;
      return [];
    });
    await statsExclusionsStore.refresh();

    const onClose = vi.fn();
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onClose,
    });

    const item = await screen.findByText("Include in Stats");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(invoke).toHaveBeenCalledWith("set_stats_excluded", {
      entityType: "album",
      entityKey: "OK Computer",
      excluded: false,
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("displays 'Include in Stats' and un-excludes artist when song belongs to excluded artist", async () => {
    let exclusions: [string, string][] = [["artist", "Radiohead"]];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        if (a.excluded) exclusions = [[a.entityType, a.entityKey]];
        else exclusions = [];
      }
      if (cmd === "get_stats_exclusions") return exclusions;
      return [];
    });
    await statsExclusionsStore.refresh();

    const onClose = vi.fn();
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onClose,
    });

    const item = await screen.findByText("Include in Stats");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(invoke).toHaveBeenCalledWith("set_stats_excluded", {
      entityType: "artist",
      entityKey: "Radiohead",
      excluded: false,
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("toggles favourite from context menu", async () => {
    vi.mocked(invoke).mockResolvedValue(1);
    const onClose = vi.fn();
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: { ...localSong, loved: 0 },
      onPlay: () => {},
      onClose,
    });

    const item = await screen.findByText("Add to favourites");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(invoke).toHaveBeenCalledWith("set_song_loved", {
      songId: 1,
      loved: 1,
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("toggles dislike from context menu", async () => {
    vi.mocked(invoke).mockResolvedValue(-1);
    const onClose = vi.fn();
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: { ...localSong, loved: 0 },
      onPlay: () => {},
      onClose,
    });

    const item = await screen.findByText("Dislike track");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(invoke).toHaveBeenCalledWith("set_song_loved", {
      songId: 1,
      loved: -1,
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("shows remove favourite and clear dislike when appropriate", async () => {
    const { unmount } = render(SongContextMenu, {
      x: 0,
      y: 0,
      song: { ...localSong, loved: 1 },
      onPlay: () => {},
      onClose: () => {},
    });

    expect(await screen.findByText("Remove from favourites")).toBeInTheDocument();
    unmount();

    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: { ...localSong, loved: -1 },
      onPlay: () => {},
      onClose: () => {},
    });

    expect(await screen.findByText("Clear dislike")).toBeInTheDocument();
  });
});
