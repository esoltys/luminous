import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast } from "../utils/colorUtils";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerRenderOptions,
} from "./types";

/**
 * A plain line trace of the spectrum bands, registered only in dev builds so the preset
 * selector can be tried by hand while the voxel spectrum is the only shipped preset.
 */
class TestPatternInstance implements VisualizerInstance {
  private bands: readonly number[] = [];
  private stroke = "#8b5cf6";

  setTheme(colors: ThemeColors): void {
    const bg = colors["bg-playerbar"] || colors["bg-main"] || "#191918";
    this.stroke = clampForContrast(colors["color-accent"] || "#8b5cf6", bg, 3.0);
  }

  push(frame: VisualizerFrame): void {
    this.bands = frame.bands;
  }

  step(): boolean {
    return false;
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void {
    if (width <= 0 || height <= 0) return;
    ctx.clearRect(0, 0, width, height);
    const n = 32;
    const mid = height / 2;
    ctx.strokeStyle = this.stroke;
    ctx.lineWidth = Math.max(1.5, height / 40);
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const v = options.preview ? 0.5 + 0.45 * Math.sin(i * 0.6) : this.bands[i] || 0;
      const x = (i / (n - 1)) * width;
      const y = mid + (i % 2 === 0 ? -1 : 1) * v * mid * 0.9;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  }
}

export const DEV_TEST_PATTERN_PRESET: VisualizerPreset = {
  id: "dev-test-pattern",
  // Dev-only, so not in the locale catalogs: i18n.t() falls back to the key itself.
  labelKey: "Test pattern (dev)",
  needs: ["spectrum"],
  create: () => new TestPatternInstance(),
};
