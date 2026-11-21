import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
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
import { visualizerStore } from "../stores/visualizer.svelte";
import { registerPreset } from "../visualizers/registry";
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
    const detailsBox = getByTestId("immersive-visualizer-song-details");
    const badge = getByText("Now playing");
    const title = getByText("Neon Horizon");
    const album = getByText("Midnight Grid");
    const artist = getByText("Synthwave Collective");

    expect(detailsBox).not.toContainElement(badge);
    expect(detailsBox).toContainElement(title);
    expect(detailsBox).toContainElement(album);
    expect(detailsBox).toContainElement(artist);
    expect(badge.compareDocumentPosition(title) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(title.compareDocumentPosition(album) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(album.compareDocumentPosition(artist) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

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

  it("turns the spectrum feed on while active and draws the selected preset", () => {
    const { getByTestId } = render(ImmersiveVisualizer, { active: true });
    expect(invoke).toHaveBeenCalledWith("set_spectrum_enabled", { enabled: true });
    expect(getByTestId("immersive-visualizer-canvas")).toHaveAttribute("data-preset", "voxel");
  });

  describe("preset switching (#2245)", () => {
    let unregister: (() => void) | null = null;

    beforeEach(() => {
      unregister = registerPreset({
        id: "fake",
        labelKey: "Fake preset",
        needs: ["spectrum"],
        create: () => ({ setTheme() {}, push() {}, step: () => false, render() {} }),
      });
    });

    afterEach(() => {
      visualizerStore.select("voxel");
      unregister?.();
      localStorage.clear();
    });

    it("steps through presets with ←/→ and redraws with the new one", async () => {
      const { getByTestId } = render(ImmersiveVisualizer, { active: true });
      await fireEvent.keyDown(window, { key: "ArrowRight" });
      expect(visualizerStore.preset.id).toBe("fake");
      expect(getByTestId("immersive-visualizer-canvas")).toHaveAttribute("data-preset", "fake");
      await fireEvent.keyDown(window, { key: "ArrowLeft" });
      expect(visualizerStore.preset.id).toBe("voxel");
    });

    it("leaves ←/→ alone while inactive, with a modifier, or in a slider or text field", async () => {
      const { unmount } = render(ImmersiveVisualizer, { active: false });
      await fireEvent.keyDown(window, { key: "ArrowRight" });
      expect(visualizerStore.preset.id).toBe("voxel");
      unmount();

      render(ImmersiveVisualizer, { active: true });
      await fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true });

      const slider = document.createElement("input");
      slider.type = "range";
      const field = document.createElement("div");
      field.setAttribute("contenteditable", "true");
      document.body.append(slider, field);
      await fireEvent.keyDown(slider, { key: "ArrowRight" });
      await fireEvent.keyDown(field, { key: "ArrowLeft" });
      slider.remove();
      field.remove();

      expect(visualizerStore.preset.id).toBe("voxel");
    });

    it("opens the preset menu on right-click of the stage", async () => {
      const { getByTestId, getAllByRole } = render(ImmersiveVisualizer, { active: true });
      await fireEvent.contextMenu(getByTestId("immersive-visualizer-canvas"));
      expect(getAllByRole("menuitemradio")).toHaveLength(2);
    });
  });
});
