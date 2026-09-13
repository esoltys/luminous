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

  it("exits Immersive Mode and navigates to album when album title is clicked while immersive (#909)", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = true;
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");
    const viewAlbumSpy = vi.spyOn(navigationStore, "viewAlbum").mockImplementation(() => {});

    const { getByText } = render(PlayerBar);
    await fireEvent.click(getByText("Test Album"));

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(viewAlbumSpy).toHaveBeenCalledWith("Test Album");
  });

  it("exits Immersive Mode and navigates to artist when artist title is clicked while immersive (#909)", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = true;
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");
    const viewArtistSpy = vi.spyOn(navigationStore, "viewArtist").mockImplementation(() => {});

    const { getByText } = render(PlayerBar);
    await fireEvent.click(getByText("Test Artist"));

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(viewArtistSpy).toHaveBeenCalledWith("Test Artist");
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

  // #876: cover art now only toggles Immersive View — navigating to the
  // Queue is the dedicated Queue button's job (see the "playbar button row"
  // tests below).

  it("toggles Immersive Mode when the album cover is clicked, regardless of the current view", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = false;
    navigationStore.activeTab = "collection";

    const toggleImmersiveModeSpy = vi.spyOn(windowLayoutStore, "toggleImmersiveMode");
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist").mockImplementation(() => {});

    const { getByTitle } = render(PlayerBar);
    const coverButton = getByTitle("Immersive Mode");
    await fireEvent.click(coverButton);

    expect(toggleImmersiveModeSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(viewPlaylistSpy).not.toHaveBeenCalled();
  });

  it("exits Immersive Mode when the album cover is clicked while immersive", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = true;

    const toggleImmersiveModeSpy = vi.spyOn(windowLayoutStore, "toggleImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    const coverButton = getByTitle("Immersive Mode");
    await fireEvent.click(coverButton);

    expect(toggleImmersiveModeSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
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
    const coverButton = getByTitle("Immersive Mode");
    await fireEvent.click(coverButton);

    expect(toggleImmersiveModeSpy).not.toHaveBeenCalled();
    expect(exitImmersiveModeSpy).not.toHaveBeenCalled();
  });

  it("shows and hides controls matching Full, Compact, and Minimal tiers across breakpoints", () => {
    // Tiers: Full (>=640px), Compact (400-640px), Minimal (<400px).
    // Full tier (>=640px):
    windowLayoutStore.viewportWidth = 1280;
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    const { getAllByTitle, getByTitle, queryByTitle, container, unmount } = render(PlayerBar);

    // Full tier has shuffle, repeat, seek row, right toolbar (with volume & miniplayer)
    const shuffleBtn = getAllByTitle(/shuffle/i).find(el => el.querySelector("svg"))!;
    const repeatBtn = getAllByTitle(/repeat/i).find(el => el.querySelector("svg"))!;
    expect(shuffleBtn).toBeInTheDocument();
    expect(repeatBtn).toBeInTheDocument();
    const volumeSlider = container.querySelector('input[type="range"]');
    expect(volumeSlider).toBeInTheDocument();
    const rightColumn = container.querySelector('[data-walkthrough-target="player-bar-toolbar"]');
    expect(rightColumn).toBeInTheDocument();
    const seekRow = Array.from(container.querySelectorAll("div")).find(d => d.className.includes("gap-2.5"))!;
    expect(seekRow).toBeInTheDocument();

    // In Full tier, only the right-column PiP toggle is rendered
    expect(getAllByTitle(/picture-in-picture/i).length).toBe(1);

    // Minimal-breakpoint class on previous:
    expect(getByTitle(/previous song/i)).toHaveClass("hidden", "min-[400px]:block");

    // Core controls: cover art, play/pause, skip-next
    expect(getByTitle("Immersive Mode")).toBeInTheDocument();
    expect(getByTitle(/^pause$/i)).toBeInTheDocument();
    expect(getByTitle(/next song/i)).toBeInTheDocument();

    unmount();

    // Compact tier (<640px):
    windowLayoutStore.viewportWidth = 600;
    const { getAllByTitle: getCompactAll, queryByTitle: queryCompactByTitle, container: compactContainer } = render(PlayerBar);

    // Shuffle, repeat, seek row, and right toolbar unmounted in Compact tier
    expect(queryCompactByTitle(/shuffle/i)).toBeNull();
    expect(queryCompactByTitle(/repeat/i)).toBeNull();
    expect(compactContainer.querySelector('input[type="range"]')).toBeNull();
    expect(compactContainer.querySelector('[data-walkthrough-target="player-bar-toolbar"]')).toBeNull();

    // Transport PiP toggle takes over in Compact tier so PiP mode is always accessible
    expect(getCompactAll(/picture-in-picture/i).length).toBe(1);
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

  it("exits Immersive Mode and opens the right panel when Info button is clicked while immersive and panel was closed (#909)", async () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.immersiveMode = true;
    windowLayoutStore.rightPanelOpen = false;
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    const infoBtn = getByTitle("Show Info Panel (Ctrl+I)");
    await fireEvent.click(infoBtn);

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(windowLayoutStore.rightPanelOpen).toBe(true);
  });

  it("exits Immersive Mode and keeps the right panel open when Info button is clicked while immersive and panel was already open (#909)", async () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.immersiveMode = true;
    windowLayoutStore.rightPanelOpen = true;
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    const infoBtn = getByTitle("Show Info Panel (Ctrl+I)");
    await fireEvent.click(infoBtn);

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(windowLayoutStore.rightPanelOpen).toBe(true);
  });

  // #876: the playbar's Menu/Lyrics/Queue button row.

  it("switches to the Lyrics tab when the Lyrics button is clicked", async () => {
    playerStore.currentSong = mockSong;
    navigationStore.activeTab = "collection";
    const { getByTitle } = render(PlayerBar);

    await fireEvent.click(getByTitle("Lyrics"));

    expect(navigationStore.activeTab).toBe("lyrics");
  });

  it("exits Immersive Mode and switches to the Lyrics tab when Lyrics button is clicked while immersive (#909)", async () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.immersiveMode = true;
    navigationStore.activeTab = "collection";
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    await fireEvent.click(getByTitle("Lyrics"));

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(navigationStore.activeTab).toBe("lyrics");
  });

  it("navigates to the Queue when the Queue button is clicked", async () => {
    playerStore.currentSong = mockSong;
    vi.spyOn(playlistsStore, "requireQueue").mockResolvedValue(mockQueue);
    const selectPlaylistSpy = vi.spyOn(playlistsStore, "selectPlaylist").mockImplementation(async () => {});
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist").mockImplementation(() => {});

    const { getByTitle } = render(PlayerBar);
    await fireEvent.click(getByTitle("Queue"));

    expect(selectPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
    expect(viewPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
  });

  it("exits Immersive Mode and navigates to the Queue when Queue button is clicked while immersive (#909)", async () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.immersiveMode = true;
    vi.spyOn(playlistsStore, "requireQueue").mockResolvedValue(mockQueue);
    const selectPlaylistSpy = vi.spyOn(playlistsStore, "selectPlaylist").mockImplementation(async () => {});
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist").mockImplementation(() => {});
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    await fireEvent.click(getByTitle("Queue"));

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(selectPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
    expect(viewPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
  });

  it("opens the current song's context menu from the Menu button", async () => {
    playerStore.currentSong = mockSong;
    const { getByTitle, getByText } = render(PlayerBar);

    await fireEvent.click(getByTitle("Song menu"));

    expect(getByText(/add to queue/i)).toBeInTheDocument();
  });

  it("exits Immersive Mode when 'Go to Artist' is clicked in the song context menu (#909)", async () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.immersiveMode = true;
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");
    const viewArtistSpy = vi.spyOn(navigationStore, "viewArtist").mockImplementation(() => {});

    const { getByTitle, getByText } = render(PlayerBar);
    await fireEvent.click(getByTitle("Song menu"));
    await fireEvent.click(getByText(/go to artist/i));

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(viewArtistSpy).toHaveBeenCalledWith("Test Artist");
  });

  it("exits Immersive Mode when 'Go to Album' is clicked in the song context menu (#909)", async () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.immersiveMode = true;
    const exitImmersiveSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");
    const viewAlbumSpy = vi.spyOn(navigationStore, "viewAlbum").mockImplementation(() => {});

    const { getByTitle, getByText } = render(PlayerBar);
    await fireEvent.click(getByTitle("Song menu"));
    await fireEvent.click(getByText(/go to album/i));

    expect(exitImmersiveSpy).toHaveBeenCalled();
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(viewAlbumSpy).toHaveBeenCalledWith("Test Album");
  });

  it("disables the Menu button when nothing is playing", () => {
    playerStore.currentSong = undefined;
    const { getByTitle } = render(PlayerBar);

    expect(getByTitle("Song menu")).toBeDisabled();
  });

  it("hides the info, lyrics, shuffle, and repeat buttons at the same breakpoint that auto-hides the right panel and spectrum (< 768px)", () => {
    playerStore.currentSong = mockSong;
    windowLayoutStore.viewportWidth = 1280;
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    const { queryByTitle, unmount } = render(PlayerBar);
    expect(queryByTitle("Show Info Panel (Ctrl+I)")).not.toBeNull();
    expect(queryByTitle("Lyrics")).not.toBeNull();
    expect(queryByTitle(/shuffle/i)).not.toBeNull();
    expect(queryByTitle(/repeat/i)).not.toBeNull();
    unmount();

    // 800px is >= 768px (md breakpoint): right panel is not auto-hidden, spectrum, info, lyrics, shuffle, and repeat remain visible
    windowLayoutStore.viewportWidth = 800;
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    const { queryByTitle: queryByTitleMedium, unmount: unmountMedium } = render(PlayerBar);
    expect(queryByTitleMedium("Show Info Panel (Ctrl+I)")).not.toBeNull();
    expect(queryByTitleMedium("Lyrics")).not.toBeNull();
    expect(queryByTitleMedium(/shuffle/i)).not.toBeNull();
    expect(queryByTitleMedium(/repeat/i)).not.toBeNull();
    unmountMedium();

    // 700px is < 768px: right panel auto-hides, Info, Lyrics, Shuffle, and Repeat buttons hide
    windowLayoutStore.viewportWidth = 700;
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(true);
    const { queryByTitle: queryByTitleNarrow } = render(PlayerBar);
    expect(queryByTitleNarrow("Show Info Panel (Ctrl+I)")).toBeNull();
    expect(queryByTitleNarrow("Lyrics")).toBeNull();
    expect(queryByTitleNarrow(/shuffle/i)).toBeNull();
    expect(queryByTitleNarrow(/repeat/i)).toBeNull();
  });
});
