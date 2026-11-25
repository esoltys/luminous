import { describe, it, expect, beforeEach, vi } from "vitest";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import ImmersiveCover from "./ImmersiveCover.svelte";
import { playerStore } from "../stores/player.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import type { Song } from "../types";

describe("ImmersiveCover.svelte (#2372)", () => {
  const song = {
    id: 42,
    source: "local_file",
    title: "Neon Horizon",
    artist: "Synthwave Collective",
    album: "Midnight Grid",
    art_embedded: false,
  } as Song;

  beforeEach(() => {
    vi.mocked(listen).mockImplementation(async () => () => {});
    vi.mocked(invoke).mockImplementation(async () => null);
    playerStore.currentSong = song;
    windowLayoutStore.isFullscreen = false;
  });

  it("keeps the fixed cover in a window and scales it with the screen in fullscreen", async () => {
    const { getByTestId } = render(ImmersiveCover);
    const frame = getByTestId("immersive-cover-frame");
    expect(frame.className).toContain("md:w-[380px]");
    expect(frame.className).not.toContain("vh");

    windowLayoutStore.isFullscreen = true;
    await tick();
    expect(frame.className).toContain("w-[min(65vh,45vw)]");
    expect(frame.className).not.toContain("380px");
    expect(getByTestId("immersive-cover-details").className).toContain("max-w-[40vw]");
  });
});
