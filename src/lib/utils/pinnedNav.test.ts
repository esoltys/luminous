import { describe, it, expect, beforeEach, vi } from "vitest";
import type { PinnedItem } from "../types";
import { getNavigablePins, toNavigablePin, autoPlaylistLabel } from "./pinnedNav";
import { navigationStore } from "../stores/navigation.svelte";
import { playerStore } from "../stores/player.svelte";
import { getPlaylistCardTheme } from "./playlistCardTheme";
import { i18n } from "../stores/i18n.svelte";
import { radioStore } from "../stores/radio.svelte";

describe("pinnedNav.ts", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
    navigationStore.activeTab = "home";
    navigationStore.selectedAlbumName = null;
    navigationStore.selectedArtistName = null;
    navigationStore.selectedPlaylistId = null;
    navigationStore.selectedAutoPlaylist = null;
    playerStore.currentSong = undefined;
  });

  const songItem: PinnedItem = {
    type: "song",
    song: {
      id: 42,
      title: "Popstar",
      artist: "Tinashe",
      path: "/music/popstar.mp3",
      art_embedded: true,
      art_automatic: "auto.jpg",
      art_manual: null,
    } as any,
  };

  const albumItem: PinnedItem = {
    type: "album",
    album: {
      album: "The ArchAndroid",
      artist: "Janelle Monáe",
      sample_song_id: 101,
      art_embedded: true,
      art_automatic: null,
      art_manual: null,
    } as any,
  };

  const artistItem: PinnedItem = {
    type: "artist",
    artist: {
      name: "Hammock",
      album_count: 5,
      song_count: 50,
    } as any,
  };

  const playlistItem: PinnedItem = {
    type: "playlist",
    playlist: {
      id: 7,
      name: "Late Night Drive",
      track_count: 15,
    } as any,
  };

  const autoPlaylistItem: PinnedItem = {
    type: "auto_playlist",
    autoPlaylist: {
      kind: "favourites",
      trackCount: 20,
    },
  };

  it("projects each PinnedItem type into a NavigablePin with correct titles and metadata", () => {
    const songPin = toNavigablePin(songItem);
    expect(songPin.id).toBe("song:42");
    expect(songPin.title).toBe("Popstar");
    expect(songPin.subtitle).toBe("Tinashe");
    expect(songPin.coverArt?.songId).toBe(42);

    const albumPin = toNavigablePin(albumItem);
    expect(albumPin.id).toBe("album:The ArchAndroid");
    expect(albumPin.title).toBe("The ArchAndroid");
    expect(albumPin.subtitle).toBe("Janelle Monáe");
    expect(albumPin.coverArt?.songId).toBe(101);

    const artistPin = toNavigablePin(artistItem);
    expect(artistPin.id).toBe("artist:Hammock");
    expect(artistPin.title).toBe("Hammock");
    expect(artistPin.artist?.name).toBe("Hammock");

    const playlistPin = toNavigablePin(playlistItem);
    expect(playlistPin.id).toBe("playlist:7");
    expect(playlistPin.title).toBe("Late Night Drive");

    const autoPin = toNavigablePin(autoPlaylistItem);
    expect(autoPin.id).toBe("auto_playlist:favourites");
    expect(autoPin.title).toBe(i18n.t("playlists.autoFavourites"));
  });

  it("opens a pinned country auto-playlist on its country and marks it active there", () => {
    const pin = toNavigablePin({
      type: "auto_playlist",
      autoPlaylist: { kind: "country", selector: "CA", playlistId: 9, trackCount: 30 },
    } as PinnedItem);
    expect(pin.id).toBe("auto_playlist:country:CA");
    const spy = vi.spyOn(navigationStore, "viewAutoPlaylist").mockImplementation(() => {});
    pin.open();
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ kind: "country", selector: "CA" }));
    spy.mockRestore();

    navigationStore.activeTab = "playlists";
    navigationStore.selectedAutoPlaylist = { kind: "country", selector: "CA" };
    expect(pin.isActive).toBe(false); // computed when projected
    expect(
      toNavigablePin({
        type: "auto_playlist",
        autoPlaylist: { kind: "country", selector: "CA", trackCount: 30 },
      } as PinnedItem).isActive,
    ).toBe(true);
    navigationStore.selectedAutoPlaylist = { kind: "country", selector: "NZ" };
    expect(
      toNavigablePin({
        type: "auto_playlist",
        autoPlaylist: { kind: "country", selector: "CA", trackCount: 30 },
      } as PinnedItem).isActive,
    ).toBe(false);
  });

  it("gives pinned auto-playlists the same icon as their card theme", () => {
    for (const kind of ["genre", "decade", "daypart"] as const) {
      const pin = toNavigablePin({
        type: "auto_playlist",
        autoPlaylist: { kind, trackCount: 3 },
      } as PinnedItem);
      const theme = getPlaylistCardTheme(kind);
      expect(pin.icon).toBe(theme.icon);
    }
  });

  it("detects active state across playerStore and navigationStore", () => {
    const songPin = toNavigablePin(songItem);
    expect(songPin.isActive).toBe(false);
    playerStore.currentSong = { id: 42 } as any;
    expect(toNavigablePin(songItem).isActive).toBe(true);

    const albumPin = toNavigablePin(albumItem);
    expect(albumPin.isActive).toBe(false);
    navigationStore.activeTab = "collection";
    navigationStore.selectedAlbumName = "The ArchAndroid";
    expect(toNavigablePin(albumItem).isActive).toBe(true);

    const artistPin = toNavigablePin(artistItem);
    expect(artistPin.isActive).toBe(false);
    navigationStore.selectedArtistName = "Hammock";
    expect(toNavigablePin(artistItem).isActive).toBe(true);

    const playlistPin = toNavigablePin(playlistItem);
    expect(playlistPin.isActive).toBe(false);
    navigationStore.activeTab = "playlists";
    navigationStore.selectedPlaylistId = 7;
    expect(toNavigablePin(playlistItem).isActive).toBe(true);

    const autoPin = toNavigablePin(autoPlaylistItem);
    expect(autoPin.isActive).toBe(false);
    navigationStore.selectedAutoPlaylist = { kind: "favourites" };
    expect(toNavigablePin(autoPlaylistItem).isActive).toBe(true);
  });

  it("invokes the correct navigation or playback method when open() is called", () => {
    const playSongSpy = vi.spyOn(playerStore, "playSong").mockImplementation(async () => {});
    const viewAlbumSpy = vi.spyOn(navigationStore, "viewAlbum").mockImplementation(() => {});
    const viewArtistSpy = vi.spyOn(navigationStore, "viewArtist").mockImplementation(() => {});
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist").mockImplementation(() => {});
    const viewAutoPlaylistSpy = vi
      .spyOn(navigationStore, "viewAutoPlaylist")
      .mockImplementation(() => {});

    toNavigablePin(songItem).open();
    expect(playSongSpy).toHaveBeenCalledWith(42);

    toNavigablePin(albumItem).open();
    expect(viewAlbumSpy).toHaveBeenCalledWith("The ArchAndroid");

    toNavigablePin(artistItem).open();
    expect(viewArtistSpy).toHaveBeenCalledWith("Hammock");

    toNavigablePin(playlistItem).open();
    expect(viewPlaylistSpy).toHaveBeenCalledWith(7);

    toNavigablePin(autoPlaylistItem).open();
    expect(viewAutoPlaylistSpy).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "favourites" }),
    );
  });

  it("projects a pinned station that plays it on open and is active while on air", () => {
    const stationItem: PinnedItem = {
      type: "station",
      station: {
        stationuuid: "uuid-jazz",
        name: "Jazz FM91",
        country: "Canada",
        favicon: "https://jazz.fm/icon.png",
        playable: true,
      } as any,
    };
    const playStationSpy = vi.spyOn(radioStore, "playStation").mockImplementation(async () => {});

    const pin = toNavigablePin(stationItem);
    expect(pin.id).toBe("station:uuid-jazz");
    expect(pin.title).toBe("Jazz FM91");
    expect(pin.subtitle).toBeUndefined();
    expect(pin.station?.stationuuid).toBe("uuid-jazz");
    expect(pin.isActive).toBe(false);

    pin.open();
    expect(playStationSpy).toHaveBeenCalledWith(stationItem.station);

    playerStore.currentSong = { id: 900, path: "radio://uuid-jazz" } as any;
    playerStore.state = "playing";
    expect(toNavigablePin(stationItem).isActive).toBe(true);
    expect(getNavigablePins([stationItem])).toHaveLength(1);
  });

  it("filters out empty playlists when projecting multiple items", () => {
    const emptyPlaylist: PinnedItem = {
      type: "playlist",
      playlist: { id: 99, name: "Empty", track_count: 0 } as any,
    };
    const emptyAutoPlaylist: PinnedItem = {
      type: "auto_playlist",
      autoPlaylist: { kind: "recently_added", trackCount: 0 },
    };

    const items = [songItem, emptyPlaylist, albumItem, emptyAutoPlaylist, artistItem];
    const navigable = getNavigablePins(items);

    expect(navigable).toHaveLength(3);
    expect(navigable.map((n) => n.id)).toEqual([
      "song:42",
      "album:The ArchAndroid",
      "artist:Hammock",
    ]);
  });
});
