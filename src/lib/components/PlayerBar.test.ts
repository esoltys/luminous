import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PlayerBar from "./PlayerBar.svelte";
import { playerStore } from "../stores/player.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import type { Song, Playlist } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

describe("PlayerBar.svelte", () => {
  const mockSong: Song = {
    id: 42,
    source: "local_file",
    filetype: "MP3",
    path: "/music/test.mp3",
    title: "Test Track Title",
    artist: "Test Artist",
    album: "Test Album",
    album_artist: "Test Artist",
    composer: "Composer",
    genre: "Rock",
    track: 1,
    disc: 1,
    year: 2024,
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
  };

  beforeEach(() => {
    vi.clearAllMocks();
    playerStore.state = "stopped";
    playerStore.currentSong = undefined;
    playerStore.volume = 0.8;
    playerStore.shuffleMode = "off";
    playerStore.repeatMode = "off";
    // Reset the viewport-driven breakpoint flags so tests that set
    // viewportWidth narrow (to exercise isImmersiveForced) don't leak into
    // later tests via the shared collectionStore singleton.
    windowLayoutStore.viewportWidth = 1280;
    windowLayoutStore.viewportHeight = 800;
    // Reset navigation + immersive state so tests that put the store into
    // "viewing the Queue" (or immersive mode) don't leak into later tests
    // via the shared collectionStore/playlistsStore singletons.
    windowLayoutStore.immersiveMode = false;
    navigationStore.activeTab = "collection";
    navigationStore.playlistsSubTab = "custom";
    navigationStore.selectedPlaylistId = null;
    playlistsStore.playlists = [];
  });

  it("renders 'Not Playing' state when currentSong is undefined", () => {
    const { getByText } = render(PlayerBar);
    expect(getByText(/nothing playing/i)).toBeInTheDocument();
  });

  it("renders song title, album title, and artist when a song is active", () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";

    const { getByText } = render(PlayerBar);
    expect(getByText("Test Track Title")).toBeInTheDocument();
    expect(getByText("Test Album")).toBeInTheDocument();
    expect(getByText("Test Artist")).toBeInTheDocument();
  });

  it("hides the album row entirely when the current song has no album (#428)", () => {
    playerStore.currentSong = { ...mockSong, album: "" };
    playerStore.state = "playing";

    const { queryByText, getByText } = render(PlayerBar);
    expect(getByText("Test Track Title")).toBeInTheDocument();
    expect(queryByText("Test Album")).not.toBeInTheDocument();
    expect(queryByText(/unknown album/i)).not.toBeInTheDocument();
  });

  it("hides the album row when the album is whitespace-only (#428)", () => {
    playerStore.currentSong = { ...mockSong, album: "   " };
    playerStore.state = "playing";

    const { queryByText } = render(PlayerBar);
    expect(queryByText(/unknown album/i)).not.toBeInTheDocument();
  });

  it("navigates to album when album title is clicked", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    const viewAlbumSpy = vi.spyOn(navigationStore, "viewAlbum").mockImplementation(() => {});

    const { getByText } = render(PlayerBar);
    const albumLink = getByText("Test Album");
    await fireEvent.click(albumLink);

    expect(viewAlbumSpy).toHaveBeenCalledWith("Test Album");
  });

  const mockQueue: Playlist = {
    id: 1,
    name: "Queue",
    dynamic_enabled: false,
    created: 0,
    updated: 0,
    track_count: 0,
    is_queue: true,
  };

  // #523: the three-state PlayerBar cover-art click flow — collection/
  // playlist view -> Queue -> Immersive Mode -> back to Queue.

  it("navigates to the Queue when the album cover is clicked from a collection/playlist view", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = false;
    navigationStore.activeTab = "collection";

    vi.spyOn(playlistsStore, "requireQueue").mockResolvedValue(mockQueue);
    const selectPlaylistSpy = vi.spyOn(playlistsStore, "selectPlaylist").mockImplementation(async () => {});
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist").mockImplementation(() => {});
    const toggleImmersiveModeSpy = vi.spyOn(windowLayoutStore, "toggleImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    const coverButton = getByTitle("Queue");
    await fireEvent.click(coverButton);

    expect(selectPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
    expect(viewPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
    expect(toggleImmersiveModeSpy).not.toHaveBeenCalled();
  });

  it("flips into Immersive Mode when the album cover is clicked while already viewing the Queue", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = false;
    playlistsStore.playlists = [mockQueue];
    navigationStore.activeTab = "playlists";
    navigationStore.playlistsSubTab = "custom";
    navigationStore.selectedPlaylistId = mockQueue.id;

    const toggleImmersiveModeSpy = vi.spyOn(windowLayoutStore, "toggleImmersiveMode");
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist").mockImplementation(() => {});

    const { getByTitle } = render(PlayerBar);
    const coverButton = getByTitle("Immersive Mode");
    await fireEvent.click(coverButton);

    expect(toggleImmersiveModeSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(viewPlaylistSpy).not.toHaveBeenCalled();
  });

  it("exits Immersive Mode and navigates to the Queue when the album cover is clicked", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = true;

    vi.spyOn(playlistsStore, "requireQueue").mockResolvedValue(mockQueue);
    const selectPlaylistSpy = vi.spyOn(playlistsStore, "selectPlaylist").mockImplementation(async () => {});
    const exitImmersiveModeSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist").mockImplementation(() => {});

    const { getByTitle } = render(PlayerBar);
    const coverButton = getByTitle("Queue");
    await fireEvent.click(coverButton);

    expect(exitImmersiveModeSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(selectPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
    expect(viewPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
  });

  it("calls playerStore.resume() when play button is clicked in paused/stopped state", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "stopped";
    const resumeSpy = vi.spyOn(playerStore, "resume").mockImplementation(async () => {});

    const { getByTitle } = render(PlayerBar);
    const playBtn = getByTitle("Play");
    await fireEvent.click(playBtn);

    expect(resumeSpy).toHaveBeenCalled();
  });

  it("calls playerStore.pause() when pause button is clicked during playback", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    const pauseSpy = vi.spyOn(playerStore, "pause").mockImplementation(async () => {});

    const { getByTitle } = render(PlayerBar);
    const pauseBtn = getByTitle(/pause/i);
    await fireEvent.click(pauseBtn);

    expect(pauseSpy).toHaveBeenCalled();
  });

  it("triggers previous and next track navigation", async () => {
    playerStore.currentSong = mockSong;
    const prevSpy = vi.spyOn(playerStore, "previous").mockImplementation(async () => {});
    const nextSpy = vi.spyOn(playerStore, "next").mockImplementation(async () => {});

    const { getByTitle } = render(PlayerBar);
    await fireEvent.click(getByTitle(/previous song/i));
    expect(prevSpy).toHaveBeenCalled();

    await fireEvent.click(getByTitle(/next song/i));
    expect(nextSpy).toHaveBeenCalled();
  });

  it("cycles shuffle modes on shuffle button click", async () => {
    const shuffleSpy = vi.spyOn(playerStore, "setShuffleMode").mockImplementation(async () => {});
    const { getAllByTitle } = render(PlayerBar);

    const shuffleBtn = getAllByTitle(/shuffle/i).find(el => el.querySelector("svg"))!;
    await fireEvent.click(shuffleBtn);

    expect(shuffleSpy).toHaveBeenCalledWith("all");
  });

  it("cycles repeat modes on repeat button click", async () => {
    const repeatSpy = vi.spyOn(playerStore, "setRepeatMode").mockImplementation(async () => {});
    const { getAllByTitle } = render(PlayerBar);

    const repeatBtn = getAllByTitle(/repeat/i).find(el => el.querySelector("svg"))!;
    await fireEvent.click(repeatBtn);

    expect(repeatSpy).toHaveBeenCalledWith("track");
  });

  it("pairs a mode-type icon beside the Shuffle/Repeat icon for disambiguated modes", () => {
    playerStore.shuffleMode = "all";
    playerStore.repeatMode = "off";
    const { getAllByTitle, rerender } = render(PlayerBar);

    const shuffleBtn = getAllByTitle(/shuffle/i).find(el => el.querySelector("svg"))!;
    const repeatBtn = getAllByTitle(/repeat/i).find(el => el.querySelector("svg"))!;

    // "all" shuffle and "off" repeat show only the base transport icon
    expect(shuffleBtn.querySelectorAll("svg").length).toBe(1);
    expect(repeatBtn.querySelectorAll("svg").length).toBe(1);

    playerStore.shuffleMode = "inside_album";
    playerStore.repeatMode = "track";
    rerender({});

    // Disambiguated modes pair a full-size type icon next to the base icon
    expect(shuffleBtn.querySelectorAll("svg").length).toBe(2);
    expect(shuffleBtn.textContent).not.toContain("IA");
    expect(repeatBtn.querySelectorAll("svg").length).toBe(2);
    expect(repeatBtn.textContent).not.toContain("AL");
  });

  it("shows the user-guide description for the active shuffle/repeat mode in the tooltip", () => {
    playerStore.shuffleMode = "artists";
    playerStore.repeatMode = "playlist";
    const { getAllByTitle } = render(PlayerBar);

    expect(getAllByTitle(/before moving to a new, randomly selected artist/i).length).toBeGreaterThan(0);
    expect(getAllByTitle(/loop the current queue or playlist indefinitely/i).length).toBeGreaterThan(0);
  });

  it("does not exit or enter immersive mode from the cover click while width has force-engaged it", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = false;
    windowLayoutStore.viewportWidth = 500; // below SMALL_BREAKPOINT_WIDTH_PX (640)

    const toggleImmersiveModeSpy = vi.spyOn(windowLayoutStore, "toggleImmersiveMode");
    const exitImmersiveModeSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    expect(windowLayoutStore.isImmersiveForced).toBe(true);
    const coverButton = getByTitle("Queue");
    await fireEvent.click(coverButton);

    expect(toggleImmersiveModeSpy).not.toHaveBeenCalled();
    expect(exitImmersiveModeSpy).not.toHaveBeenCalled();
  });

  it("marks controls with the Full/Compact/Minimal responsive classes matching their tier", () => {
    // jsdom doesn't evaluate CSS media queries, so this checks the
    // structural markup (the responsive classes are present on the right
    // elements) rather than actual visibility at a given width — real
    // breakpoint behavior is covered by manual verification. Tiers: Full
    // (>=700px), Compact (400-700px), Minimal (<400px) — cover art, play/
    // pause, and skip-next are the constant core shown in all three.
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    const { getAllByTitle, getByTitle, container } = render(PlayerBar);

    // Gone by Compact (Full-only): shuffle, repeat, the seek row, and the
    // volume/mute controls in the right column — the transport block takes
    // the freed space instead, sticking to the right edge via ml-auto. The
    // miniplayer toggle itself must stay visible at every tier (issue: it
    // should never disappear), so it has two instances that swap places at
    // the 700px boundary: one alongside volume/mute (Full only) and one in
    // the transport row (Compact/Minimal only, hidden again once the
    // right-column instance takes over).
    const shuffleBtn = getAllByTitle(/shuffle/i).find(el => el.querySelector("svg"))!;
    const repeatBtn = getAllByTitle(/repeat/i).find(el => el.querySelector("svg"))!;
    expect(shuffleBtn.closest(".hidden")).toHaveClass("min-[700px]:block");
    expect(repeatBtn.closest(".hidden")).toHaveClass("min-[700px]:block");
    const volumeSlider = container.querySelector('input[type="range"]');
    expect(volumeSlider).toHaveClass("hidden", "min-[700px]:block");
    const [transportToggle, rightColumnToggle] = getAllByTitle(/picture-in-picture/i);
    expect(transportToggle).toHaveClass("min-[700px]:hidden");
    expect(rightColumnToggle.closest(".hidden")).toHaveClass("min-[700px]:flex");
    const rightColumn = Array.from(container.querySelectorAll("div")).find(d => d.className.includes("justify-end"))!;
    expect(rightColumn).toHaveClass("hidden", "min-[700px]:flex");
    const seekRow = Array.from(container.querySelectorAll("div")).find(d => d.className.includes("gap-2.5"))!;
    expect(seekRow).toHaveClass("hidden", "min-[700px]:flex");

    // Gone by Minimal (Compact-and-up only): previous.
    expect(getByTitle(/previous song/i)).toHaveClass("hidden", "min-[400px]:block");

    // Never hidden (the constant core): cover art, play/pause, skip-next.
    const coverButton = getByTitle("Queue");
    expect(coverButton.closest(".hidden")).toBeNull();
    expect(getByTitle(/^pause$/i)).not.toHaveClass("hidden");
    expect(getByTitle(/next song/i)).not.toHaveClass("hidden");
  });

  it("handles mute toggle correctly", async () => {
    playerStore.volume = 0.8;
    const volSpy = vi.spyOn(playerStore, "setVolume").mockImplementation(async (v) => { playerStore.volume = v; });

    const { getByRole } = render(PlayerBar);
    const volumeBtn = getByRole("button", { name: /^volume$/i });

    // Mute
    await fireEvent.click(volumeBtn);
    expect(volSpy).toHaveBeenCalledWith(0.0);

    // Unmute
    await fireEvent.click(volumeBtn);
    expect(volSpy).toHaveBeenCalledWith(0.8);
  });

  it("toggles the right (info) panel from the button above the miniplayer toggle", async () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.rightPanelOpen = false;
    const { getByTitle } = render(PlayerBar);

    const infoBtn = getByTitle("Show Info Panel (Ctrl+I)");
    await fireEvent.click(infoBtn);
    expect(windowLayoutStore.rightPanelOpen).toBe(true);
  });

  it("hides the info-panel toggle at the same breakpoint that auto-hides the panel itself", () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.viewportWidth = 1280;
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    const { queryByTitle } = render(PlayerBar);
    expect(queryByTitle("Show Info Panel (Ctrl+I)")).not.toBeNull();

    windowLayoutStore.viewportWidth = 800;
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(true);
    const { queryByTitle: queryByTitleNarrow } = render(PlayerBar);
    expect(queryByTitleNarrow("Show Info Panel (Ctrl+I)")).toBeNull();
  });
});
