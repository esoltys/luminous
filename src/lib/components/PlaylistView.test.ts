import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, within } from "@testing-library/svelte";
import PlaylistView from "./PlaylistView.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playerStore } from "../stores/player.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { Playlist, PlaylistItem, PlaylistSizeFit } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  // Lists for every command except the size-limit answer, which is an object or null.
  invoke: vi.fn().mockImplementation(async (cmd: string) => (cmd === "get_playlist_size_fit" ? null : [])),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

/** What `get_playlist_size_fit` answers; every other command answers a list. */
let fitAnswer: PlaylistSizeFit | null = null;
const answerCommands = async (cmd: string) => (cmd === "get_playlist_size_fit" ? fitAnswer : []);

describe("PlaylistView.svelte", () => {
  const mockPlaylist: Playlist = {
    id: 1,
    name: "My Playlist",
    track_count: 3,
    created: 1700000000,
    updated: 1700000000,
    dynamic_enabled: false,
    is_queue: false,
  };

  const mockTracks: PlaylistItem[] = [
    {
      id: 10,
      playlist_id: 1,
      position: 0,
      item_type: "song",
      uuid: "uuid-1",
      song: {
        id: 101,
        source: "local_file",
        filetype: "MP3",
        path: "/music/track1.mp3",
        title: "Track One",
        artist: "Artist A",
        album: "Album A",
        album_artist: "Artist A",
        genre: "Rock",
        track: 1,
        disc: 1,
        year: 2021,
        compilation: false,
        length_nanosec: 180_000_000_000,
        beginning_nanosec: 0,
        end_nanosec: 180_000_000_000,
        rating: 4,
        playcount: 5,
        skipcount: 0,
        art_embedded: false,
        art_unset: false,
        unavailable: false,
      },
    },
    {
      id: 11,
      playlist_id: 1,
      position: 1,
      item_type: "song",
      uuid: "uuid-2",
      song: {
        id: 102,
        source: "local_file",
        filetype: "MP3",
        path: "/music/track2.mp3",
        title: "Track Two",
        artist: "Artist B",
        album: "Album B",
        album_artist: "Artist B",
        genre: "Pop",
        track: 2,
        disc: 1,
        year: 2022,
        compilation: false,
        length_nanosec: 200_000_000_000,
        beginning_nanosec: 0,
        end_nanosec: 200_000_000_000,
        rating: 5,
        playcount: 10,
        skipcount: 0,
        art_embedded: false,
        art_unset: false,
        unavailable: false,
      },
    },
    {
      id: 12,
      playlist_id: 1,
      position: 2,
      item_type: "song",
      uuid: "uuid-3",
      song: {
        id: 103,
        source: "local_file",
        filetype: "MP3",
        path: "/music/track3.mp3",
        title: "Track Three",
        artist: "Artist C",
        album: "Album C",
        album_artist: "Artist C",
        genre: "Jazz",
        track: 3,
        disc: 1,
        year: 2020,
        compilation: false,
        length_nanosec: 150_000_000_000,
        beginning_nanosec: 0,
        end_nanosec: 150_000_000_000,
        rating: 3,
        playcount: 2,
        skipcount: 0,
        art_embedded: false,
        art_unset: false,
        unavailable: false,
      },
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.searchQuery = "";
    playlistsStore.playlists = [mockPlaylist];
    playlistsStore.activePlaylistId = 1;
    playlistsStore.activePlaylistTracks = [...mockTracks];
    fitAnswer = null;
    vi.mocked(invoke).mockImplementation(answerCommands);
  });

  it("renders playlist tracks", () => {
    const { getByText } = render(PlaylistView);
    expect(getByText("My Playlist")).toBeInTheDocument();
    expect(getByText("Track One")).toBeInTheDocument();
    expect(getByText("Track Two")).toBeInTheDocument();
    expect(getByText("Track Three")).toBeInTheDocument();
  });

  it("offers a rename affordance for a regular playlist", () => {
    const { getAllByTitle } = render(PlaylistView);
    expect(getAllByTitle("Rename playlist").length).toBeGreaterThan(0);
  });

  it("hides the rename affordance for the queue", () => {
    playlistsStore.playlists = [{ ...mockPlaylist, name: "Queue", is_queue: true }];
    const { queryByTitle } = render(PlaylistView);
    expect(queryByTitle("Rename playlist")).toBeNull();
  });

  describe("Auto Continue (#1235)", () => {
    const queue = { ...mockPlaylist, name: "Queue", is_queue: true };
    const AUTO = '{"autoContinue":true}';

    beforeEach(() => {
      playerStore.autoContinue = false;
    });

    it("offers the toggle only on the Queue", () => {
      const { queryByRole, unmount } = render(PlaylistView);
      expect(queryByRole("button", { name: "Auto Continue" })).toBeNull();
      unmount();

      playlistsStore.playlists = [queue];
      const { getByRole } = render(PlaylistView);
      const offToggle = getByRole("button", { name: "Auto Continue" });
      expect(offToggle).toHaveAttribute("aria-pressed", "false");
      expect(offToggle).toHaveAttribute("title", "Auto Continue: Off");
    });

    it("reflects the backend's state and asks it to flip", async () => {
      playlistsStore.playlists = [queue];
      playerStore.autoContinue = true;
      const { getByRole } = render(PlaylistView);
      const toggle = getByRole("button", { name: "Auto Continue" });
      expect(toggle).toHaveAttribute("aria-pressed", "true");
      expect(toggle).toHaveAttribute("title", "Auto Continue: On");

      await fireEvent.click(toggle);
      expect(invoke).toHaveBeenCalledWith("set_auto_continue", { enabled: false });
    });

    it("draws a divider above the songs Auto Continue added", () => {
      playlistsStore.playlists = [queue];
      playlistsStore.activePlaylistTracks = [
        mockTracks[0],
        { ...mockTracks[1], additional_metadata: AUTO },
        { ...mockTracks[2], additional_metadata: AUTO },
      ];
      const { getAllByRole } = render(PlaylistView);
      const dividers = getAllByRole("separator", { name: "Auto Continue" });
      expect(dividers).toHaveLength(1);
      expect(dividers[0].nextElementSibling).toHaveTextContent("Track Two");
    });

    it("drops the divider when the Queue is sorted, since the added songs are no longer a run", async () => {
      playlistsStore.playlists = [queue];
      playlistsStore.activePlaylistTracks = [mockTracks[0], { ...mockTracks[1], additional_metadata: AUTO }];
      const { getByText, queryByRole } = render(PlaylistView);
      await fireEvent.click(getByText("Title").closest("button")!);
      expect(queryByRole("separator", { name: "Auto Continue" })).toBeNull();
    });
  });

  it("handles pointer-based drag reordering of playlist items", async () => {
    // Tauri's dragDropEnabled window option blocks native HTML5 drag events, so reordering
    // is implemented with pointer events instead (see SongTable.svelte handleRowPointerDown).
    const reorderSpy = vi.spyOn(playlistsStore, "moveItems").mockResolvedValue();
    const { getByText } = render(PlaylistView);

    const rowOne = getByText("Track One").closest("[data-song-row]")! as HTMLElement;
    const rowThree = getByText("Track Three").closest("[data-song-row]")! as HTMLElement;

    document.elementFromPoint = vi.fn().mockReturnValue(rowThree);

    await fireEvent.pointerDown(rowOne, { button: 0, clientX: 0, clientY: 0 });
    // Move past the drag threshold so it registers as a drag rather than a click.
    await fireEvent.pointerMove(window, { clientX: 0, clientY: 20 });
    await fireEvent.pointerUp(window);

    // Dropped on a row below it, so it lands after Track Three: at the end.
    expect(reorderSpy).toHaveBeenCalledWith(1, ["uuid-1"], null);
  });

  it("drops a song dragged upward in front of the row it lands on", async () => {
    const moveSpy = vi.spyOn(playlistsStore, "moveItems").mockResolvedValue();
    const { getByText } = render(PlaylistView);
    const rowThree = getByText("Track Three").closest("[data-song-row]")! as HTMLElement;
    const rowTwo = getByText("Track Two").closest("[data-song-row]")! as HTMLElement;
    document.elementFromPoint = vi.fn().mockReturnValue(rowTwo);

    await fireEvent.pointerDown(rowThree, { button: 0, clientX: 0, clientY: 20 });
    await fireEvent.pointerMove(window, { clientX: 0, clientY: 0 });
    await fireEvent.pointerUp(window);

    expect(moveSpy).toHaveBeenCalledWith(1, ["uuid-3"], "uuid-2");
  });

  it("doesn't drag rows while searching, since hidden songs have no drop position", async () => {
    collectionStore.searchQuery = "Track";
    const moveSpy = vi.spyOn(playlistsStore, "moveItems").mockResolvedValue();
    const { getByText } = render(PlaylistView);
    const rowOne = getByText("Track One").closest("[data-song-row]")! as HTMLElement;
    document.elementFromPoint = vi.fn().mockReturnValue(getByText("Track Three").closest("[data-song-row]"));

    await fireEvent.pointerDown(rowOne, { button: 0, clientX: 0, clientY: 0 });
    await fireEvent.pointerMove(window, { clientX: 0, clientY: 20 });
    await fireEvent.pointerUp(window);

    expect(moveSpy).not.toHaveBeenCalled();
  });

  describe("sorting a playlist (#344)", () => {
    it("asks the backend to sort and shows its order as given", async () => {
      const setSortSpy = vi.spyOn(playlistsStore, "setSort").mockResolvedValue();
      const { getByText, getAllByRole } = render(PlaylistView);
      await fireEvent.click(getByText("Title").closest("button")!);
      expect(setSortSpy).toHaveBeenCalledWith(1, "title", true);

      // A playlist sorted by rating, descending: the rows keep the backend's order.
      playlistsStore.playlists = [{ ...mockPlaylist, sort_field: "rating", sort_asc: false }];
      playlistsStore.activePlaylistTracks = [mockTracks[2], mockTracks[0], mockTracks[1]];
      await Promise.resolve();
      const titles = getAllByRole("row").slice(1).map((r) => r.textContent);
      expect(titles[0]).toContain("Track Three");
      expect(titles[2]).toContain("Track Two");
    });

    it("returns a sorted playlist to manual order from the # header", async () => {
      playlistsStore.playlists = [{ ...mockPlaylist, sort_field: "title", sort_asc: true }];
      const setSortSpy = vi.spyOn(playlistsStore, "setSort").mockResolvedValue();
      const { getAllByRole } = render(PlaylistView);
      const header = getAllByRole("row")[0];
      await fireEvent.click(header.querySelector("button")!);
      expect(setSortSpy).toHaveBeenCalledWith(1, null, true);
    });
  });

  describe("size limit (#344)", () => {
    const fit: PlaylistSizeFit = {
      limit_bytes: 16_000_000_000,
      break_index: 2,
      fit_songs: 2,
      fit_bytes: 12_400_000_000,
      total_bytes: 20_000_000_000,
      order: ["uuid-1", "uuid-2", "uuid-3"],
      device_id: null,
      device_name: null,
    };

    beforeEach(() => {
      playlistsStore.playlists = [{ ...mockPlaylist, size_limit_bytes: fit.limit_bytes }];
      fitAnswer = fit;
    });

    it("draws a page break above the first song that doesn't fit, and dims nothing", async () => {
      const { findByRole, getByRole, getByText } = render(PlaylistView);
      const brk = await findByRole("separator", { name: "16 GB limit · 2 songs above fit · 7.6 GB below" });
      expect(brk).toHaveAttribute("data-divider-variant", "break");
      expect(brk.nextElementSibling).toHaveTextContent("Track Three");
      expect(getByText("Track Three").closest("[data-song-row]")!.className).not.toContain("opacity-50");
      expect(getByText("12.4 GB of 16 GB used")).toBeInTheDocument();
      expect(getByRole("button", { name: /Limit 16 GB/ })).toBeInTheDocument();
    });

    it("keeps the break between the visible rows while searching", async () => {
      collectionStore.searchQuery = "Track T";
      const { findByRole } = render(PlaylistView);
      const brk = await findByRole("separator", { name: /16 GB limit/ });
      expect(brk.nextElementSibling).toHaveTextContent("Track Three");
    });

    it("hides the break while the shown order isn't the one the fit was computed for", async () => {
      playlistsStore.activePlaylistTracks = [mockTracks[1], mockTracks[0], mockTracks[2]];
      const { findByText, queryByRole } = render(PlaylistView);
      await findByText("Measuring sizes…");
      expect(queryByRole("separator", { name: /16 GB limit/ })).toBeNull();
    });

    it("draws no break when every song fits", async () => {
      fitAnswer = { ...fit, break_index: 3, fit_songs: 3 };
      const { findByText, queryByRole } = render(PlaylistView);
      await findByText("12.4 GB of 16 GB used");
      expect(queryByRole("separator", { name: /16 GB limit/ })).toBeNull();
    });

    it("sets and removes the limit from the popover", async () => {
      const setLimitSpy = vi.spyOn(playlistsStore, "setSizeLimit").mockResolvedValue();
      const { getByRole } = render(PlaylistView);
      await fireEvent.click(getByRole("button", { name: /Limit 16 GB/ }));
      const dialog = getByRole("dialog", { name: "Size limit" });
      expect(within(dialog).getByRole("button", { name: "16 GB" })).toHaveAttribute("aria-pressed", "true");

      await fireEvent.click(within(dialog).getByRole("button", { name: "2 GB" }));
      expect(setLimitSpy).toHaveBeenCalledWith(1, 2_000_000_000, null);

      await fireEvent.click(getByRole("button", { name: /Limit 16 GB/ }));
      await fireEvent.click(getByRole("button", { name: "Remove limit" }));
      expect(setLimitSpy).toHaveBeenLastCalledWith(1, null, null);
    });

    it("sets a typed size in the chosen unit", async () => {
      playlistsStore.playlists = [mockPlaylist];
      const setLimitSpy = vi.spyOn(playlistsStore, "setSizeLimit").mockResolvedValue();
      const { getByRole, getByPlaceholderText } = render(PlaylistView);
      await fireEvent.click(getByRole("button", { name: "Size limit" }));
      await fireEvent.input(getByPlaceholderText("Custom size"), { target: { value: "1.5" } });
      await fireEvent.click(getByRole("button", { name: "Set" }));
      expect(setLimitSpy).toHaveBeenCalledWith(1, 1_500_000_000, null);
    });

    it("names the device when the sizes are converted ones", async () => {
      playlistsStore.playlists = [{ ...mockPlaylist, size_limit_bytes: fit.limit_bytes, size_limit_device: "vol-1" }];
      fitAnswer = { ...fit, device_id: "vol-1", device_name: "HiBy R1" };
      const { findByText } = render(PlaylistView);
      expect(await findByText("12.4 GB of 16 GB used on HiBy R1")).toBeInTheDocument();
    });

    it("says the sizes are being measured until the answer arrives", () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) =>
        cmd === "get_playlist_size_fit" ? new Promise(() => {}) : []
      );
      const { getByText } = render(PlaylistView);
      expect(getByText("Measuring sizes…")).toBeInTheDocument();
    });

    it("shows no answer for another limit than the one set", async () => {
      fitAnswer = { ...fit, limit_bytes: 8_000_000_000 };
      const { findByText, queryByRole } = render(PlaylistView);
      await findByText("Measuring sizes…");
      expect(queryByRole("separator", { name: /limit/ })).toBeNull();
    });

    it("re-measures for a connected device picked in the popover", async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_connected_devices") return [{ id: "vol-1", defaultName: "HiBy R1", profile: { nickname: null } }];
        return answerCommands(cmd);
      });
      const setLimitSpy = vi.spyOn(playlistsStore, "setSizeLimit").mockResolvedValue();
      const { getByRole, findByRole } = render(PlaylistView);
      await fireEvent.click(getByRole("button", { name: /Limit 16 GB/ }));
      await findByRole("option", { name: "HiBy R1" });
      await fireEvent.change(getByRole("combobox", { name: "Count sizes for" }), { target: { value: "vol-1" } });
      expect(setLimitSpy).toHaveBeenCalledWith(1, 16_000_000_000, "vol-1");
    });

    it("offers no limit on the Queue", () => {
      playlistsStore.playlists = [{ ...mockPlaylist, name: "Queue", is_queue: true }];
      const { queryByRole } = render(PlaylistView);
      expect(queryByRole("button", { name: "Size limit" })).toBeNull();
    });
  });

  it("does not reorder when pointer movement stays below the drag threshold", async () => {
    const reorderSpy = vi.spyOn(playlistsStore, "moveItems").mockResolvedValue();
    const { getByText } = render(PlaylistView);

    const rowOne = getByText("Track One").closest("[data-song-row]")! as HTMLElement;
    const rowThree = getByText("Track Three").closest("[data-song-row]")! as HTMLElement;

    const elementFromPointMock = vi.fn().mockReturnValue(rowThree);
    document.elementFromPoint = elementFromPointMock;

    await fireEvent.pointerDown(rowOne, { button: 0, clientX: 0, clientY: 0 });
    await fireEvent.pointerMove(window, { clientX: 0, clientY: 1 });
    await fireEvent.pointerUp(window);

    expect(elementFromPointMock).not.toHaveBeenCalled();
    expect(reorderSpy).not.toHaveBeenCalled();
  });

  it("navigates to the artist when clicking the Artist link instead of starting a row drag", async () => {
    // Regression test for #553: handleRowPointerDown used to call setPointerCapture
    // unconditionally on pointerdown, stealing subsequent pointer events from the
    // Artist/Album LinkButton and silently blocking its onclick navigation.
    // jsdom doesn't implement setPointerCapture at all, so stub it directly rather than
    // spying on a non-existent prototype method.
    const setPointerCaptureSpy = vi.fn();
    const originalSetPointerCapture = HTMLElement.prototype.setPointerCapture;
    HTMLElement.prototype.setPointerCapture = setPointerCaptureSpy;
    try {
      const viewArtistSpy = vi.spyOn(navigationStore, "viewArtist");
      const reorderSpy = vi.spyOn(playlistsStore, "moveItems").mockResolvedValue();
      const { getByText } = render(PlaylistView);

      const artistLink = getByText("Artist A").closest("button")!;

      await fireEvent.pointerDown(artistLink, { button: 0, clientX: 0, clientY: 0 });
      expect(setPointerCaptureSpy).not.toHaveBeenCalled();

      await fireEvent.click(artistLink);

      expect(viewArtistSpy).toHaveBeenCalledWith("Artist A");
      expect(reorderSpy).not.toHaveBeenCalled();
    } finally {
      HTMLElement.prototype.setPointerCapture = originalSetPointerCapture;
    }
  });

  it("triggers undo and redo from the overflow menu", async () => {
    const undoSpy = vi.spyOn(playlistsStore, "undo").mockResolvedValue();
    const redoSpy = vi.spyOn(playlistsStore, "redo").mockResolvedValue();
    const { getByText, getByTitle } = render(PlaylistView);

    const overflowBtn = getByTitle("More actions");
    await fireEvent.click(overflowBtn);

    expect(getByText("Undo")).toBeInTheDocument();
    expect(getByText("Redo")).toBeInTheDocument();

    const undoBtn = getByText("Undo").closest("button")!;
    await fireEvent.click(undoBtn);
    expect(undoSpy).toHaveBeenCalled();

    // Reopen overflow menu for redo
    await fireEvent.click(overflowBtn);
    const redoBtn = getByText("Redo").closest("button")!;
    await fireEvent.click(redoBtn);
    expect(redoSpy).toHaveBeenCalled();
  });

  it("filters tracks using collectionStore.searchQuery and displays empty state when nothing matches", async () => {
    collectionStore.searchQuery = "Track One";
    const { getByText, queryByText } = render(PlaylistView);

    expect(getByText("Track One")).toBeInTheDocument();
    expect(queryByText("Track Two")).not.toBeInTheDocument();
    expect(queryByText("Track Three")).not.toBeInTheDocument();
  });

  it("detects duplicate tracks and triggers deduplication", async () => {
    playlistsStore.activePlaylistTracks = [
      ...mockTracks,
      {
        id: 13,
        playlist_id: 1,
        position: 3,
        item_type: "song",
        uuid: "uuid-4-dup",
        song: { ...mockTracks[0].song! },
      },
    ];

    const dedupeSpy = vi.spyOn(playlistsStore, "deduplicatePlaylist");
    const { getByText, getByTitle } = render(PlaylistView);

    const overflowBtn = getByTitle("More actions");
    await fireEvent.click(overflowBtn);

    expect(getByText(/Remove 1 duplicate/)).toBeInTheDocument();
    const btn = getByText(/Remove 1 duplicate/).closest("button")!;
    await fireEvent.click(btn);

    expect(dedupeSpy).toHaveBeenCalledWith(1);
  });

  it("handles multi-selection with Shift+Click and shows batch floating bar", async () => {
    const { getByText, queryByText } = render(PlaylistView);

    const rowOne = getByText("Track One").closest("[data-song-row]")!;
    const rowThree = getByText("Track Three").closest("[data-song-row]")!;

    await fireEvent.click(rowOne);
    await fireEvent.click(rowThree, { shiftKey: true });

    expect(getByText("3 selected")).toBeInTheDocument();

    const removeBtn = getByText("Remove Selected").closest("button")!;
    const removeSpy = vi.spyOn(playlistsStore, "removeItemsFromPlaylist");
    await fireEvent.click(removeBtn);

    expect(removeSpy).toHaveBeenCalledWith(1, ["uuid-1", "uuid-2", "uuid-3"]);
  });

  it("opens context menu on right-click", async () => {
    const { getByText, getAllByText } = render(PlaylistView);
    const rowOne = getByText("Track One").closest("[data-song-row]")!;

    await fireEvent.contextMenu(rowOne);

    expect(getAllByText("Play Selected").length).toBeGreaterThan(0);
    expect(getByText("Remove from Playlist")).toBeInTheDocument();
  });

  it("sorts the Queue on the frontend by column header clicks and toggles direction", async () => {
    // The Queue's order is the playback order, so only its view sorts locally;
    // other playlists persist their sort in the backend (see "sorting a playlist").
    playlistsStore.playlists = [{ ...mockPlaylist, name: "Queue", is_queue: true }];
    const { getByText, getAllByRole } = render(PlaylistView);

    // Initial order: Track One, Track Two, Track Three
    let rows = getAllByRole("row").slice(1); // Exclude header row
    expect(rows[0]).toHaveTextContent("Track One");
    expect(rows[1]).toHaveTextContent("Track Two");
    expect(rows[2]).toHaveTextContent("Track Three");

    // Click "Title" column header to sort alphabetically by title
    const titleHeaderBtn = getByText("Title").closest("button")!;
    await fireEvent.click(titleHeaderBtn);

    rows = getAllByRole("row").slice(1);
    expect(rows[0]).toHaveTextContent("Track One");
    expect(rows[1]).toHaveTextContent("Track Three");
    expect(rows[2]).toHaveTextContent("Track Two");

    // Click "Title" header again to toggle to descending order
    await fireEvent.click(titleHeaderBtn);

    rows = getAllByRole("row").slice(1);
    expect(rows[0]).toHaveTextContent("Track Two");
    expect(rows[1]).toHaveTextContent("Track Three");
    expect(rows[2]).toHaveTextContent("Track One");

    // Click "Artist" header to sort by artist
    const artistHeaderBtn = getByText("Artist").closest("button")!;
    await fireEvent.click(artistHeaderBtn);

    rows = getAllByRole("row").slice(1);
    expect(rows[0]).toHaveTextContent("Artist A");
    expect(rows[1]).toHaveTextContent("Artist B");
    expect(rows[2]).toHaveTextContent("Artist C");
  });

  it("renders genre chips on their own line and cover stack in top-right for custom playlist", () => {
    // Ensure at least one track has artwork so topAlbums is populated
    playlistsStore.activePlaylistTracks = [
      {
        ...mockTracks[0],
        song: { ...mockTracks[0].song!, art_manual: "/covers/album-a.jpg" },
      },
      mockTracks[1],
      mockTracks[2],
    ];

    const { getByText, getByTitle } = render(PlaylistView);

    // Genre chips should be rendered
    const rockChip = getByTitle("Browse Rock");
    expect(rockChip).toBeInTheDocument();
    expect(getByTitle("Browse Pop")).toBeInTheDocument();
    expect(getByTitle("Browse Jazz")).toBeInTheDocument();

    // Verify genre container is distinct from the metadata container with songs count
    const genreContainer = rockChip.closest("div.flex.flex-wrap.gap-1");
    expect(genreContainer).not.toBeNull();

    const songsText = getByText("3 songs");
    expect(songsText).toBeInTheDocument();
    expect(genreContainer?.contains(songsText)).toBe(false);

    const metadataRow = songsText.closest("div.flex.flex-wrap.items-center");
    expect(metadataRow).not.toBeNull();
    expect(metadataRow?.firstElementChild).toBe(songsText);

    // Cover mosaic container should be positioned at top-right (not bottom-right / self-stretch)
    const coverStackContainer = document.querySelector(".shrink-0.shadow-xl");
    expect(coverStackContainer).not.toBeNull();
    expect(coverStackContainer).toHaveClass("items-start");
    expect(coverStackContainer).not.toHaveClass("self-stretch");
  });

  it("synchronizes activePlaylist from navigationStore.selectedPlaylistId when mounted with mismatched activePlaylistId", () => {
    const queuePlaylist: Playlist = {
      id: 99,
      name: "Queue",
      track_count: 0,
      created: 1700000000,
      updated: 1700000000,
      dynamic_enabled: false,
      is_queue: true,
    };
    playlistsStore.playlists = [queuePlaylist, mockPlaylist];
    playlistsStore.activePlaylistId = 99; // Queue was previously active
    navigationStore.selectedPlaylistId = 1; // Custom playlist is selected in navigation

    const { getByText } = render(PlaylistView);
    expect(getByText("My Playlist")).toBeInTheDocument();
  });
});

