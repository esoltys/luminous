import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumDetailView from "./AlbumDetailView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playerStore } from "../stores/player.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { picardStore } from "../stores/picard.svelte";
import { prefs } from "../stores/prefs.svelte";
import { tasksStore } from "../stores/tasks.svelte";
import { toastStore } from "../stores/toast.svelte";
import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    innerSize: () => Promise.resolve({ width: 800, height: 600 }),
    outerSize: () => Promise.resolve({ width: 800, height: 600 }),
    listen: vi.fn().mockResolvedValue(() => {}),
    onResized: vi.fn().mockResolvedValue(() => {}),
    onMoved: vi.fn().mockResolvedValue(() => {}),
  }),
}));

describe("AlbumDetailView.svelte - Play vs Shuffle Play Queue navigation", () => {
  const mockAlbumName = "Abbey Road";
  const mockSongs = [
    {
      id: 1,
      title: "Come Together",
      artist: "The Beatles",
      album: "Abbey Road",
      length_nanosec: 259_000_000_000,
    },
    {
      id: 2,
      title: "Something",
      artist: "The Beatles",
      album: "Abbey Road",
      length_nanosec: 183_000_000_000,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    navigationStore.selectedAlbumName = mockAlbumName;
    navigationStore.activeTab = "collection";
    collectionStore.albums = [];
    playlistsStore.playlists = [
      { id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false, is_queue: true },
    ];

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve(mockSongs);
      }
      if (cmd === "get_all_app_settings") {
        return Promise.resolve({});
      }
      if (cmd === "get_playlists") {
        return Promise.resolve([{ id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false, is_queue: true }]);
      }
      if (cmd === "create_playlist") {
        return Promise.resolve({ id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false, is_queue: true });
      }
      if (cmd === "get_playlist_items") {
        return Promise.resolve([]);
      }
      if (cmd === "set_shuffle_mode" || cmd === "play_songs") {
        return Promise.resolve(undefined);
      }
      return Promise.resolve([]);
    });
  });

  it("does not switch views to Queue when user clicks Play", async () => {
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist");
    const playSongsSpy = vi.spyOn(playerStore, "playSongs");
    const setShuffleSpy = vi.spyOn(playerStore, "setShuffleMode");

    const { getByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    // Wait for songs to load
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Find the header "Play" button exactly
    const playButton = getByText("Play").closest("button")!;
    await fireEvent.click(playButton);
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(setShuffleSpy).toHaveBeenCalledWith("off");
    expect(playSongsSpy).toHaveBeenCalledWith([1, 2], 0, undefined, {
      type: "album",
      album: mockAlbumName,
      albumArtist: "The Beatles",
    });
    expect(viewPlaylistSpy).not.toHaveBeenCalled();
    expect(navigationStore.activeTab).toBe("collection");
  });

  it("retains view on Shuffle while updating playback and queue", async () => {
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist");
    const playSongsSpy = vi.spyOn(playerStore, "playSongs");
    const setShuffleSpy = vi.spyOn(playerStore, "setShuffleMode");

    const { getByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    // Wait for songs to load
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Find the "Shuffle" button exactly
    const shuffleButton = getByText("Shuffle").closest("button")!;
    await fireEvent.click(shuffleButton);
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(setShuffleSpy).toHaveBeenCalledWith("off");
    expect(playSongsSpy).toHaveBeenCalled();
    expect(viewPlaylistSpy).not.toHaveBeenCalled();
    expect(navigationStore.activeTab).toBe("collection");
  });

  it("adds all album songs to Queue when clicking the + button with no active custom playlist", async () => {
    playlistsStore.activeCustomPlaylist = null;
    const addSongsSpy = vi.spyOn(playlistsStore, "addSongsToQueue");

    const { getByTitle } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    const addButton = getByTitle("Add all songs to Queue");
    await fireEvent.click(addButton);
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(addSongsSpy).toHaveBeenCalledWith([1, 2]);
    expect(invoke).toHaveBeenCalledWith("add_songs_to_queue", { songIds: [1, 2] });
  });

  it("renders genre chips on their own line and begins year on the next line", async () => {
    const songsWithGenreAndYear = [
      {
        id: 1,
        title: "Song 1",
        artist: "Krisu",
        album: "Oxygen for a Dying World",
        genre: "Electronic; Chill Out; Trip-Hop; Lounge",
        year: 2024,
        length_nanosec: 259_000_000_000,
      },
    ];

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve(songsWithGenreAndYear);
      }
      return Promise.resolve([]);
    });

    const { getByText, getAllByText } = render(AlbumDetailView, {
      props: { albumName: "Oxygen for a Dying World" },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    // Verify genre chips are rendered
    const electronicChip = getByText("Electronic");
    const chillOutChip = getByText("Chill Out");
    expect(electronicChip).toBeInTheDocument();
    expect(chillOutChip).toBeInTheDocument();

    // Verify genre container is distinct from the metadata container with year
    const genreContainer = electronicChip.closest("div.flex.flex-wrap.gap-1");
    expect(genreContainer).not.toBeNull();

    const yearElements = getAllByText("2024");
    expect(yearElements.length).toBeGreaterThan(0);
    const headerYear = yearElements[0];
    expect(headerYear).toBeInTheDocument();
    expect(genreContainer?.contains(headerYear)).toBe(false);

    // The metadata row starts with the year
    const metadataRow = headerYear.closest("div.flex.flex-wrap.items-center");
    expect(metadataRow).not.toBeNull();
    expect(metadataRow?.firstElementChild).toBe(headerYear);
  });

  it("opens overflow menu with a single Edit Album Details entry and Open in Picard", async () => {
    picardStore.path = "/mock/picard";
    const { getByTitle, getByText, queryByText, getAllByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(queryByText("Open in Picard")).toBeNull();

    const moreBtn = getByTitle("More actions");
    await fireEvent.click(moreBtn);
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Regression test for #962: there used to be two near-identical "Edit
    // Album..." entries here (one writing embedded genre, one writing a
    // separate curated tag list) that silently disagreed with each other.
    // Now there's exactly one.
    const editItems = getAllByText("Edit Album Details");
    expect(editItems.length).toBe(1);
    const picardItem = getByText("Open in Picard");
    expect(picardItem).toBeInTheDocument();

    await fireEvent.click(picardItem);
    expect(invoke).toHaveBeenCalledWith("open_in_picard", { songIds: [1, 2] });
  });

  it("opens the album's CritiqueBrainz release group page from the overflow menu (#1387)", async () => {
    const openUrl = vi.fn().mockResolvedValue(undefined);
    vi.doMock("@tauri-apps/plugin-opener", () => ({ openUrl }));
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return [{ ...mockSongs[0], musicbrainz_release_group_id: "rg-123" }];
      }
      return [];
    });
    const { getByTitle, getByText } = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
    await new Promise((resolve) => setTimeout(resolve, 50));
    await fireEvent.click(getByTitle("More actions"));
    await new Promise((resolve) => setTimeout(resolve, 50));
    await fireEvent.click(getByText("Review on CritiqueBrainz"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(openUrl).toHaveBeenCalledWith("https://critiquebrainz.org/release-group/rg-123");
    vi.doUnmock("@tauri-apps/plugin-opener");
  });

  it("disables Review on CritiqueBrainz when no release group MBID is tagged (#1387)", async () => {
    const { getByTitle, getByText } = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
    await new Promise((resolve) => setTimeout(resolve, 50));
    await fireEvent.click(getByTitle("More actions"));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(getByText("Review on CritiqueBrainz").closest("button")).toBeDisabled();
  });

  it("shows the CritiqueBrainz community rating in place of the Album Info title (#1387)", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_songs_by_album") return [{ ...mockSongs[0], musicbrainz_release_group_id: "rg-123" }];
      if (cmd === "get_song_context") return { critiquebrainz_rating: 4.2, critiquebrainz_review_count: 7, critiquebrainz_review_links: [] };
      return [];
    });
    collectionStore.albumProfiles = {
      "abbey road": { album_key: "abbey road", artist_key: "the beatles", description: "Classic album", links: [] },
    };
    const { findByText } = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
    expect(await findByText("(7)")).toBeInTheDocument();
  });

  it("shows the community rating on its own when the album has no Album Info content (#1387)", async () => {
    collectionStore.albumProfiles = {};
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_songs_by_album") return [{ ...mockSongs[0], musicbrainz_release_group_id: "rg-123" }];
      if (cmd === "get_song_context") return { critiquebrainz_rating: 3.5, critiquebrainz_review_count: 2, critiquebrainz_review_links: [] };
      return [];
    });
    const { findByText } = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
    expect(await findByText("(2)")).toBeInTheDocument();
  });

  it("toggles album stats exclusion from the overflow menu (#1252)", async () => {
    let excluded: [string, string][] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        excluded = a.excluded ? [[a.entityType, a.entityKey]] : [];
      }
      if (cmd === "get_stats_exclusions") return excluded;
      if (cmd === "get_songs_by_album") return mockSongs;
      if (cmd === "get_all_app_settings") return {};
      return [];
    });
    await statsExclusionsStore.refresh();

    const { getByTitle, getByText, queryByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });
    await new Promise((resolve) => setTimeout(resolve, 50));

    const moreBtn = getByTitle("More actions");
    await fireEvent.click(moreBtn);
    await new Promise((resolve) => setTimeout(resolve, 50));

    const excludeItem = getByText("Don't Include in Stats");
    expect(excludeItem).toBeInTheDocument();

    await fireEvent.click(excludeItem);
    await vi.waitFor(() => expect(statsExclusionsStore.isExcluded("album", mockAlbumName)).toBe(true));
    expect(invoke).toHaveBeenCalledWith("set_stats_excluded", {
      entityType: "album",
      entityKey: mockAlbumName,
      excluded: true,
    });

    // Reopen menu to verify label flips to "Include in Stats"
    await fireEvent.click(moreBtn);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(getByText("Include in Stats")).toBeInTheDocument();
    expect(queryByText("Don't Include in Stats")).toBeNull();
  });

  it("scopes Album Info card to group/overview, buttons to group/link, and shows domain-only for unrecognized sites (#1133)", async () => {
    collectionStore.albumProfiles = {
      "abbey road": {
        album_key: "abbey road",
        artist_key: "the beatles",
        description: "Classic album",
        website: "https://thebeatles.com",
        links: [
          {
            platform: "other_databases",
            handle_or_url: "https://vgmdb.net/album/12345/",
          },
          {
            platform: "allmusic",
            handle_or_url: "https://www.allmusic.com/album/mw0000192938",
          },
        ],
      },
    };

    const { getByText, container } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    // Album Info card should exist and use group/overview rather than un-namespaced group
    const details = container.querySelector("details");
    expect(details).not.toBeNull();
    expect(details?.classList.contains("group/overview")).toBe(true);
    expect(details?.classList.contains("group")).toBe(false);

    // Unrecognized / other_databases link must show only domain name
    expect(getByText("vgmdb.net")).toBeInTheDocument();
    // Branded platform link shows its label
    expect(getByText("AllMusic")).toBeInTheDocument();

    // Release link buttons should use group/link scoping
    const vgmdbButton = getByText("vgmdb.net").closest("button")!;
    expect(vgmdbButton.classList.contains("group/link")).toBe(true);
    expect(vgmdbButton.classList.contains("group")).toBe(false);

    // External link icon should have group-hover/link:opacity-100
    const extIcon = vgmdbButton.querySelector("svg.opacity-0");
    expect(extIcon?.classList.contains("group-hover/link:opacity-100")).toBe(true);
    expect(extIcon?.classList.contains("group-hover:opacity-100")).toBe(false);
  });

  it("does not show blacklisted links (rateyourmusic.com, twitter.com, x.com) in Album Info", async () => {
    collectionStore.albumProfiles = {
      "abbey road": {
        album_key: "abbey road",
        artist_key: "the beatles",
        description: "Classic album",
        website: "https://thebeatles.com",
        links: [
          {
            platform: "other_databases",
            handle_or_url: "https://rateyourmusic.com/release/album/the-beatles/abbey-road/",
          },
          {
            platform: "custom",
            handle_or_url: "https://twitter.com/thebeatles",
          },
          {
            platform: "x",
            handle_or_url: "https://x.com/thebeatles",
          },
          {
            platform: "allmusic",
            handle_or_url: "https://www.allmusic.com/album/mw0000192938",
          },
        ],
      },
    };

    const { getByText, queryByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(queryByText("rateyourmusic.com")).toBeNull();
    expect(queryByText("twitter.com")).toBeNull();
    expect(queryByText("x.com")).toBeNull();
    expect(queryByText("X (Twitter)")).toBeNull();
    expect(getByText("AllMusic")).toBeInTheDocument();
    expect(getByText("thebeatles.com")).toBeInTheDocument();
  });

  it("auto-fetches album details on visit when details_fetched is false and context enrichment is enabled (#1143)", async () => {
    const invokeMock = vi.mocked(invoke);
    collectionStore.albumProfiles = {
      "abbey road": {
        album_key: "abbey road",
        details_fetched: false,
        links: [],
      },
    };

    const songsWithMbid = [
      {
        id: 1,
        title: "Come Together",
        artist: "The Beatles",
        album: "Abbey Road",
        musicbrainz_release_group_id: "rg-123",
      },
    ];

    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") return Promise.resolve(songsWithMbid);
      if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
      if (cmd === "retrieve_album_details") {
        return Promise.resolve({
          added_count: 1,
          profile: { album_key: "Abbey Road", details_fetched: true, links: [] },
        });
      }
      return Promise.resolve();
    });

    render(AlbumDetailView, { props: { albumName: mockAlbumName } });

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("retrieve_album_details", { album: "Abbey Road" });
    });
    // The automatic pass also backfills fanart.tv art, but only what's missing (#1277).
    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("retrieve_album_art", { album: "Abbey Road", onlyMissing: true });
    });
  });

  it("does not toast when the automatic album details fetch fails", async () => {
    const invokeMock = vi.mocked(invoke);
    collectionStore.albumProfiles = {
      "abbey road": { album_key: "abbey road", details_fetched: false, links: [] },
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve([
          { id: 1, title: "Come Together", artist: "The Beatles", album: "Abbey Road", musicbrainz_release_group_id: "rg-123" },
        ]);
      }
      if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
      if (cmd === "retrieve_album_details") return Promise.reject("error sending request for url");
      return Promise.resolve();
    });
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    render(AlbumDetailView, { props: { albumName: mockAlbumName } });

    await vi.waitFor(() => expect(warnSpy).toHaveBeenCalled());
    expect(tasksStore.tasks.some((t) => t.id === "album-enrichment-abbey road")).toBe(false);
    expect(toastStore.messages.some((m) => m.task?.taskId === "album-enrichment-abbey road")).toBe(false);
    warnSpy.mockRestore();
  });

  it("does not auto-fetch album details when details_fetched is already true (#1143)", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockClear();
    collectionStore.albumProfiles = {
      "abbey road": {
        album_key: "abbey road",
        details_fetched: true,
        links: [],
      },
    };

    const songsWithMbid = [
      {
        id: 1,
        title: "Come Together",
        artist: "The Beatles",
        album: "Abbey Road",
        musicbrainz_release_group_id: "rg-123",
      },
    ];

    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") return Promise.resolve(songsWithMbid);
      if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
      return Promise.resolve();
    });

    render(AlbumDetailView, { props: { albumName: mockAlbumName } });
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(invokeMock).not.toHaveBeenCalledWith("retrieve_album_details", expect.anything());
  });

  describe("song picked from search (#1280)", () => {
    const scrollIntoView = vi.fn();
    beforeEach(() => {
      scrollIntoView.mockClear();
      Element.prototype.scrollIntoView = scrollIntoView;
    });

    it("selects the focused song's row, scrolls it into view, and clears the signal", async () => {
      navigationStore.pendingFocusSongId = 2;
      const { container } = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
      await new Promise((resolve) => setTimeout(resolve, 50));

      const row = container.querySelector<HTMLElement>('[data-song-row][data-key="2"]')!;
      expect(row.className).toContain("bg-brand-accent/20");
      expect(container.querySelector('[data-song-row][data-key="1"]')!.className).not.toContain("bg-brand-accent/20");
      expect(scrollIntoView).toHaveBeenCalledTimes(1);
      expect(scrollIntoView.mock.contexts[0]).toBe(row);
      expect(navigationStore.pendingFocusSongId).toBeNull();
    });

    it("clears the signal without selecting anything when the song isn't on this album", async () => {
      navigationStore.pendingFocusSongId = 999;
      const { container } = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
      await new Promise((resolve) => setTimeout(resolve, 50));

      for (const row of container.querySelectorAll("[data-song-row]")) {
        expect(row.className).not.toContain("bg-brand-accent/20");
      }
      expect(scrollIntoView).not.toHaveBeenCalled();
      expect(navigationStore.pendingFocusSongId).toBeNull();
    });
  });

  function renderWithFetchedDetails(profile: Record<string, unknown>) {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockClear();
    collectionStore.albumProfiles = {
      "abbey road": { album_key: "abbey road", details_fetched: true, links: [], ...profile },
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve([
          { id: 1, title: "Come Together", artist: "The Beatles", album: "Abbey Road", musicbrainz_release_group_id: "rg-123" },
        ]);
      }
      if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
      if (cmd === "retrieve_album_art") {
        return Promise.resolve({
          cover_uri: null,
          disc_uri: null,
          profile: { album_key: "abbey road", details_fetched: true, links: [], cover_fetched: true, disc_fetched: true },
        });
      }
      return Promise.resolve();
    });
    const view = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
    return { invokeMock, view };
  }

  it("backfills only fanart.tv art when details were already fetched (#1277)", async () => {
    const { invokeMock } = renderWithFetchedDetails({ cover_fetched: true, disc_fetched: false });

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("retrieve_album_art", { album: "Abbey Road", onlyMissing: true });
    });
    expect(invokeMock).not.toHaveBeenCalledWith("retrieve_album_details", expect.anything());
  });

  it("does not backfill fanart.tv art that was attempted or whose type is off (#1277)", async () => {
    prefs.fanartFetchDiscArt = false;
    try {
      const { invokeMock } = renderWithFetchedDetails({ cover_fetched: true, disc_fetched: false });
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(invokeMock).not.toHaveBeenCalledWith("retrieve_album_art", expect.anything());
    } finally {
      prefs.fanartFetchDiscArt = true;
    }
  });

  it("fetches every fanart.tv art type from a manual Retrieve Album Details (#1277)", async () => {
    const { invokeMock, view } = renderWithFetchedDetails({ cover_fetched: true, disc_fetched: true });
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve([
          { id: 1, title: "Come Together", artist: "The Beatles", album: "Abbey Road", musicbrainz_release_group_id: "rg-123" },
        ]);
      }
      if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
      if (cmd === "retrieve_album_details") {
        return Promise.resolve({ added_count: 0, profile: { album_key: "abbey road", details_fetched: true, links: [] } });
      }
      if (cmd === "retrieve_album_art") {
        return Promise.resolve({ cover_uri: null, disc_uri: null, profile: { album_key: "abbey road", details_fetched: true, links: [] } });
      }
      return Promise.resolve();
    });

    await new Promise((resolve) => setTimeout(resolve, 50));
    await fireEvent.click(view.getByTitle("More actions"));
    await fireEvent.click(await view.findByText("Retrieve Album Details"));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("retrieve_album_art", { album: "Abbey Road", onlyMissing: false });
    });
  });

  it("clicking Refresh Album in overflow menu invokes rescan_songs with album songs, not full library scan", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve([
          { id: 1, title: "Come Together", artist: "The Beatles", album: "Abbey Road", disc: 1, track: 1 },
          { id: 2, title: "Something", artist: "The Beatles", album: "Abbey Road", disc: 1, track: 2 },
        ]);
      }
      if (cmd === "rescan_songs") return Promise.resolve();
      return Promise.resolve();
    });

    const refreshLibrarySpy = vi.spyOn(collectionStore, "refreshLibrary").mockResolvedValue(undefined as any);
    const startScanSpy = vi.spyOn(collectionStore, "startScan");

    const view = render(AlbumDetailView, { props: { albumName: mockAlbumName } });
    await new Promise((resolve) => setTimeout(resolve, 50));

    await fireEvent.click(view.getByTitle("More actions"));
    const refreshBtn = await view.findByText("Refresh Album");
    expect(refreshBtn).toBeTruthy();
    await fireEvent.click(refreshBtn);

    expect(invokeMock).toHaveBeenCalledWith("rescan_songs", { songIds: [1, 2] });
    expect(refreshLibrarySpy).toHaveBeenCalled();
    expect(startScanSpy).not.toHaveBeenCalled();
  });
});
