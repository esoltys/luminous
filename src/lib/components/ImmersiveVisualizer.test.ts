import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import ImmersiveVisualizer from "./ImmersiveVisualizer.svelte";
import { playerStore } from "../stores/player.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import { visualizerStore } from "../stores/visualizer.svelte";
import { registerPreset } from "../visualizers/registry.svelte";
import type { Song } from "../types";
import type { SpectrumFrame } from "../utils/spectrumEnable";
import type { VisualizerFrame, VisualizerTrack } from "../visualizers/types";

describe("ImmersiveVisualizer.svelte (#2236)", () => {
  const mockSong: Song = {
    id: 42,
    source: "local_file",
    filetype: "FLAC",
    path: "/music/test.flac",
    title: "Neon Horizon",
    artist: "Synthwave Collective",
    album: "Midnight Grid",
    album_artist: "Synthwave Collective",
    composer: "Synthwave Collective",
    genre: "Electronic",
    year: 2026,
    track: 1,
    disc: 1,
    compilation: false,
    length_nanosec: 210_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 210_000_000_000,
    rating: 5,
    playcount: 3,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockImplementation(async () => () => {});
    vi.mocked(invoke).mockImplementation(async () => null);
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    windowLayoutStore.immersiveMode = true;
    windowLayoutStore.setImmersiveView("visualizer");
  });

  it("renders the compact song header and voxel visualizer canvas, and switches to cover view when the thumbnail is clicked", async () => {
    const { getByTestId, getByText, getByTitle, queryByText } = render(ImmersiveVisualizer, {
      active: true,
    });

    expect(getByTestId("immersive-visualizer")).toBeInTheDocument();
    expect(getByTestId("immersive-visualizer-canvas")).toBeInTheDocument();
    const detailsBox = getByTestId("immersive-visualizer-song-details");
    const title = getByText("Neon Horizon");
    const album = getByText("Midnight Grid");
    const artist = getByText("Synthwave Collective");

    // The header is the song itself, with no "Now playing" chip above it.
    expect(queryByText("Now playing")).toBeNull();
    expect(detailsBox).toContainElement(title);
    expect(detailsBox).toContainElement(album);
    expect(detailsBox).toContainElement(artist);
    expect(title.compareDocumentPosition(album) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(album.compareDocumentPosition(artist) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    const thumbBtn = getByTitle("Immersive Mode");
    await fireEvent.click(thumbBtn);

    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveView).toBe("cover");
  });

  it("keeps descenders of the leading-none truncated title and artist lines inside their clip box (#2250)", () => {
    const { getByText } = render(ImmersiveVisualizer, { active: true });
    // jsdom has no layout, so assert the mechanism: bottom padding extends the overflow clip box
    // below the 1em line box, and the matching negative margin keeps the header height unchanged.
    for (const line of [getByText("Neon Horizon"), getByText("Synthwave Collective")]) {
      expect(line).toHaveClass("leading-none", "truncate", "pb-[0.2em]", "-mb-[0.2em]");
    }
  });

  it("fills the available container without fixed pixel caps so large windows expand the stage (#2259)", () => {
    const { getByTestId } = render(ImmersiveVisualizer, { active: true });
    const root = getByTestId("immersive-visualizer");
    expect(root).toHaveClass("w-full", "h-full", "@container");
    expect(root).not.toHaveClass("max-w-5xl", "max-h-[600px]");
  });

  it("subscribes to spectrum-data events and renders on the canvas when active", async () => {
    let spectrumCallback: ((event: { payload: SpectrumFrame }) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (eventName, cb) => {
      if (eventName === "spectrum-data") {
        spectrumCallback = cb as (event: { payload: SpectrumFrame }) => void;
      }
      return () => {};
    });

    const { getByTestId } = render(ImmersiveVisualizer, { active: true });
    expect(getByTestId("immersive-visualizer-canvas")).toBeInTheDocument();
    expect(listen).toHaveBeenCalledWith("spectrum-data", expect.any(Function));

    spectrumCallback?.({ payload: { bands: new Array(32).fill(0.8), onset: 0 } });
  });

  it("turns the spectrum feed on while active and draws the selected preset", () => {
    const { getByTestId } = render(ImmersiveVisualizer, { active: true });
    expect(invoke).toHaveBeenCalledWith("set_spectrum_enabled", { enabled: true });
    expect(getByTestId("immersive-visualizer-canvas")).toHaveAttribute("data-preset", "voxel");
  });

  // The built-in voxel spectrum and voxel logo (#2252) are the two presets to switch between.
  describe("preset switching (#2245)", () => {
    afterEach(() => {
      visualizerStore.select("voxel");
      localStorage.clear();
    });

    it("redraws with the newly selected preset", async () => {
      const { getByTestId } = render(ImmersiveVisualizer, { active: true });
      visualizerStore.cycle(1);
      await tick();
      expect(getByTestId("immersive-visualizer-canvas")).toHaveAttribute(
        "data-preset",
        "voxel-iso",
      );
    });

    it("hands the preset each frame's onset exactly once", async () => {
      const pushed: VisualizerFrame[] = [];
      const unregisterSpy = registerPreset({
        id: "spy",
        labelKey: "Spy preset",
        needs: ["spectrum"],
        create: () => ({
          setTheme() {},
          push: (frame) => void pushed.push(frame),
          step: () => false,
          render() {},
        }),
      });
      let send: ((frame: SpectrumFrame) => void) | undefined;
      vi.mocked(listen).mockImplementation(async (eventName, cb) => {
        if (eventName === "spectrum-data") {
          send = (frame) => (cb as (e: { payload: SpectrumFrame }) => void)({ payload: frame });
        }
        return () => {};
      });
      try {
        visualizerStore.select("spy");
        render(ImmersiveVisualizer, { active: true });
        await tick();
        await tick();
        send?.({ bands: new Array(32).fill(0.5), onset: 0.7 });
        expect(pushed.at(-1)?.onset).toBe(0.7);
        // A later push without new data (playback stopping) carries no stale onset.
        playerStore.state = "paused";
        await tick();
        expect(pushed.at(-1)?.onset).toBe(0);
      } finally {
        unregisterSpy();
      }
    });

    it("hands the preset the track and its cover colours as they change (#2248)", async () => {
      const tracks: (VisualizerTrack | null)[] = [];
      const unregisterSpy = registerPreset({
        id: "track-spy",
        labelKey: "Track spy",
        needs: ["spectrum"],
        create: () => ({
          setTheme() {},
          setTrack: (track) => void tracks.push(track),
          push() {},
          step: () => false,
          render() {},
        }),
      });
      try {
        visualizerStore.select("track-spy");
        render(ImmersiveVisualizer, { active: true });
        await tick();
        expect(tracks.at(-1)).toEqual({ key: "42", accent: null });
        const handedOver = tracks.length;

        // Another copy of the same song is not a new track.
        playerStore.currentSong = { ...mockSong };
        await tick();
        expect(tracks).toHaveLength(handedOver);

        playerStore.currentSong = { ...mockSong, id: 43 };
        await tick();
        expect(tracks.at(-1)).toEqual({ key: "43", accent: null });
      } finally {
        playerStore.currentSong = mockSong;
        unregisterSpy();
      }
    });

    it("opens the preset menu on right-click of the stage", async () => {
      const { getByTestId, getAllByRole } = render(ImmersiveVisualizer, { active: true });
      await fireEvent.contextMenu(getByTestId("immersive-visualizer-canvas"));
      expect(getAllByRole("menuitemradio")).toHaveLength(6);
    });
  });
});
