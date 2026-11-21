import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
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
import type { Song } from "../types";

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
    const { getByTestId, getByText, getByTitle } = render(ImmersiveVisualizer, {
      active: true,
    });

    expect(getByTestId("immersive-visualizer")).toBeInTheDocument();
    expect(getByTestId("immersive-visualizer-canvas")).toBeInTheDocument();
    expect(getByText("Neon Horizon")).toBeInTheDocument();
    expect(getByText("Midnight Grid")).toBeInTheDocument();
    expect(getByText("Synthwave Collective")).toBeInTheDocument();

    const thumbBtn = getByTitle("Immersive Mode");
    await fireEvent.click(thumbBtn);

    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveView).toBe("cover");
  });

  it("subscribes to spectrum-data events and renders on the canvas when active", async () => {
    let spectrumCallback: ((event: { payload: number[] }) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (eventName, cb) => {
      if (eventName === "spectrum-data") {
        spectrumCallback = cb as (event: { payload: number[] }) => void;
      }
      return () => {};
    });

    const { getByTestId } = render(ImmersiveVisualizer, { active: true });
    expect(getByTestId("immersive-visualizer-canvas")).toBeInTheDocument();
    expect(listen).toHaveBeenCalledWith("spectrum-data", expect.any(Function));

    spectrumCallback?.({ payload: new Array(32).fill(0.8) });
  });
});
