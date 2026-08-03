import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Miniplayer from "./Miniplayer.svelte";
import { playerStore } from "../stores/player.svelte";
import { collectionStore } from "../stores/collection.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn().mockReturnValue({
    startResizing: vi.fn().mockResolvedValue(undefined),
    startDragging: vi.fn().mockResolvedValue(undefined),
  }),
}));

describe("Miniplayer.svelte", () => {
  const mockSong: Song = {
    id: 101,
    source: "local_file",
    filetype: "FLAC",
    path: "/music/ambient.flac",
    title: "Starlight Echoes",
    artist: "Lunar Drift",
    album: "Solaris",
    album_artist: "Lunar Drift",
    genre: "Ambient",
    compilation: false,
    length_nanosec: 240_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 240_000_000_000,
    rating: 5,
    playcount: 12,
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
    collectionStore.isMiniplayer = true;
  });

  it("renders idle layout with current song title and artist", () => {
    playerStore.currentSong = mockSong;
    const { getAllByText } = render(Miniplayer);
    expect(getAllByText("Starlight Echoes").length).toBeGreaterThan(0);
    expect(getAllByText("Lunar Drift").length).toBeGreaterThan(0);
  });

  it("toggles play/pause when play button is clicked", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "paused";
    const resumeSpy = vi.spyOn(playerStore, "resume").mockResolvedValue();

    const { getByTitle } = render(Miniplayer);
    const playBtn = getByTitle("Play");
    await fireEvent.click(playBtn);

    expect(resumeSpy).toHaveBeenCalled();
  });

  it("cycles shuffle and repeat modes pairing a mode-type icon beside the transport icon", async () => {
    playerStore.currentSong = mockSong;
    const { getByTitle } = render(Miniplayer);

    const shuffleBtn = getByTitle(/Shuffle:/i);
    const repeatBtn = getByTitle(/Repeat:/i);

    // off / off: only the base transport icon
    expect(shuffleBtn.querySelectorAll("svg").length).toBe(1);
    expect(repeatBtn.querySelectorAll("svg").length).toBe(1);

    // Click shuffle -> all (still no type icon) -> inside_album (DiscAlbum pairs with Shuffle)
    await fireEvent.click(shuffleBtn); // all
    expect(shuffleBtn.querySelectorAll("svg").length).toBe(1);
    await fireEvent.click(shuffleBtn); // inside_album
    expect(shuffleBtn.querySelectorAll("svg").length).toBe(2);

    // Click repeat -> track (Music pairs with Repeat) -> album (DiscAlbum pairs with Repeat)
    await fireEvent.click(repeatBtn); // track
    expect(repeatBtn.querySelectorAll("svg").length).toBe(2);
    await fireEvent.click(repeatBtn); // album
    expect(repeatBtn.querySelectorAll("svg").length).toBe(2);
  });

  it("shows the user-guide description for the active shuffle/repeat mode in the tooltip", () => {
    playerStore.currentSong = mockSong;
    playerStore.shuffleMode = "artists";
    playerStore.repeatMode = "playlist";
    const { getByTitle } = render(Miniplayer);

    expect(getByTitle(/before moving to a new, randomly selected artist/i)).toBeInTheDocument();
    expect(getByTitle(/loop the current queue or playlist indefinitely/i)).toBeInTheDocument();
  });

  it("renders the song rating widget and rates the current song", async () => {
    playerStore.currentSong = mockSong; // rating: 5 -> favorited under the default heart style
    const rateSpy = vi.spyOn(playerStore, "rateCurrent").mockResolvedValue(undefined as any);

    const { getByTitle } = render(Miniplayer);
    const heartBtn = getByTitle("Remove from favourites");
    await fireEvent.click(heartBtn);

    expect(rateSpy).toHaveBeenCalledWith(-1);
  });

  it("exits miniplayer mode when restore button is clicked or Escape is pressed", async () => {
    playerStore.currentSong = mockSong;
    const exitSpy = vi.spyOn(collectionStore, "exitMiniplayerMode");

    const { getByTitle, getByRole } = render(Miniplayer);
    const restoreBtn = getByTitle(/Restore Full Window/i);
    await fireEvent.click(restoreBtn);

    expect(exitSpy).toHaveBeenCalled();

    const region = getByRole("group");
    await fireEvent.keyDown(region, { key: "Escape" });
    expect(exitSpy).toHaveBeenCalledTimes(2);
  });

  it("adjusts volume via the slider and mutes/unmutes via the volume button", async () => {
    playerStore.currentSong = mockSong;
    const setVolumeSpy = vi.spyOn(playerStore, "setVolume");

    const { getByTitle, getByLabelText } = render(Miniplayer);

    const slider = getByLabelText("Volume Slider") as HTMLInputElement;
    await fireEvent.input(slider, { target: { value: "0.4" } });
    expect(setVolumeSpy).toHaveBeenCalledWith(0.4);

    const muteBtn = getByTitle("Volume");
    await fireEvent.click(muteBtn);
    expect(setVolumeSpy).toHaveBeenCalledWith(0.0);

    await fireEvent.click(muteBtn);
    expect(setVolumeSpy).toHaveBeenLastCalledWith(0.4);
  });

  it("handles mouse enter/leave and applies opaque background on Linux", async () => {
    playerStore.currentSong = mockSong;
    const { getByRole, container } = render(Miniplayer);

    const group = getByRole("group");
    const hoverMask = container.querySelector(".absolute.inset-0.z-30") as HTMLElement;
    expect(hoverMask).toBeInTheDocument();

    // Initially pointer events none and hidden
    expect(hoverMask.className).toContain("opacity-0");

    window.innerWidth = 1000;
    window.innerHeight = 800;

    // Pointer enter inside window bounds activates hover mask
    await fireEvent.pointerEnter(group, { clientX: 100, clientY: 100 });
    expect(hoverMask.className).toContain("opacity-100");

    // Pointer leave deactivates hover mask
    await fireEvent.pointerLeave(group);
    expect(hoverMask.className).toContain("opacity-0");

    // Opaque background class bg-brand-main is used on Linux, acrylic blur on other platforms
    if (/linux/i.test(navigator.userAgent) || /linux/i.test(navigator.platform)) {
      expect(hoverMask.className).toContain("bg-brand-main");
    } else {
      expect(hoverMask.className).toContain("bg-brand-main/85");
      expect(hoverMask.className).toContain("backdrop-blur-md");
    }
  });
});

