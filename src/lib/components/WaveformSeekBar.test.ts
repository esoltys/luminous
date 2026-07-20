import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import WaveformSeekBar from "./WaveformSeekBar.svelte";
import { themeStore } from "../stores/theme.svelte";
import { prefs } from "../stores/prefs.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_waveform_data") return Promise.resolve([10, 50, 100, 200, 150]);
    if (cmd === "get_moodbar_data") return Promise.resolve([100, 150, 200, 50, 80, 120]);
    return Promise.resolve(null);
  }),
}));

describe("WaveformSeekBar.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.seekBarMode = "waveform";
  });

  it("mounts canvas element without crashing", () => {
    const { container } = render(WaveformSeekBar);
    const canvas = container.querySelector("canvas");
    expect(canvas).toBeInTheDocument();
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
