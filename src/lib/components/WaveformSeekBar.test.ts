import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import { flushSync } from "svelte";
import WaveformSeekBar from "./WaveformSeekBar.svelte";
import { themeStore } from "../stores/theme.svelte";
import { prefs } from "../stores/prefs.svelte";
import { playerStore } from "../stores/player.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_waveform_data") return Promise.resolve([10, 50, 100, 200, 150]);
    if (cmd === "get_band_waveform_data") return Promise.resolve([100, 150, 200, 50, 80, 120]);
    return Promise.resolve(null);
  }),
}));

describe("WaveformSeekBar.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.seekBarMode = "waveform";
    playerStore.currentSong = undefined;
  });

  it("draws a bar per fetched waveform sample once data resolves", async () => {
    playerStore.currentSong = { id: 42, length_nanosec: 1000 } as any;

    const fillRectSpy = vi.fn();
    const roundRectSpy = vi.fn();
    const mockCtx = {
      clearRect: vi.fn(),
      scale: vi.fn(),
      beginPath: vi.fn(),
      roundRect: roundRectSpy,
      fill: vi.fn(),
      fillRect: fillRectSpy,
      createLinearGradient: vi.fn().mockReturnValue({ addColorStop: vi.fn() }),
      fillStyle: "",
    };
    const originalGetContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue(mockCtx) as any;

    try {
      const { container } = render(WaveformSeekBar);
      expect(container.querySelector("canvas")).toBeInTheDocument();

      // Let the async get_waveform_data invoke resolve and the resulting draw() run.
      await vi.waitFor(() => {
        flushSync();
        expect(roundRectSpy.mock.calls.length + fillRectSpy.mock.calls.length).toBeGreaterThanOrEqual(5);
      });
    } finally {
      HTMLCanvasElement.prototype.getContext = originalGetContext;
    }
  });

  it("resolves valid hex colors from themeStore for canvas drawing without invalid CSS var() strings", async () => {
    themeStore.setTheme("dynamic-artwork");

    const addColorStopSpy = vi.fn();
    const mockCtx = {
      clearRect: vi.fn(),
      scale: vi.fn(),
      beginPath: vi.fn(),
      roundRect: vi.fn(),
      fill: vi.fn(),
      fillRect: vi.fn(),
      createLinearGradient: vi.fn().mockReturnValue({
        addColorStop: addColorStopSpy,
      }),
      fillStyle: "",
    };

    // Spy on getContext to verify canvas 2D context drawing
    const originalGetContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue(mockCtx) as any;

    try {
      render(WaveformSeekBar);

      // Verify that addColorStop received concrete hex color strings, NOT 'var(...)' references
      const colorsPassed = addColorStopSpy.mock.calls.map((call) => call[1]);
      for (const color of colorsPassed) {
        expect(color).not.toContain("var(");
        expect(color).toMatch(/^#|^rgb/);
      }
    } finally {
      HTMLCanvasElement.prototype.getContext = originalGetContext;
    }
  });
});
