import { describe, it, expect, vi } from "vitest";
import { LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "../stores/theme.svelte";
import { calculateContrastRatio } from "../utils/colorUtils";
import {
  VoxelSpectrumEngine,
  buildVoxelThemePalette,
  renderStaticVoxelPreview,
  renderVoxelMatrix,
} from "./voxelEngine";
import { VOXEL_PRESET } from "./voxel";

describe("VoxelSpectrumEngine (#2236)", () => {
  it("upsamples 32 spectrum bins smoothly into 64 columns and pushes peak-hold blocks to the apex", () => {
    const engine = new VoxelSpectrumEngine(64, 28);
    const bins = Array.from({ length: 32 }, (_, i) => (i === 10 ? 1.0 : 0.2));

    engine.pushSpectrum(bins);
    const active = engine.step(50);

    expect(active).toBe(true);
    expect(engine.columns).toHaveLength(64);

    // Column mapped near bin 10 (around col 20) should be pushed high, and its interpolated neighbor should sit smoothly between 0.2 and 1.0
    const peakCol = engine.columns[20];
    const neighborCol = engine.columns[19];
    const baseCol = engine.columns[0];

    expect(peakCol.activeCells).toBeGreaterThan(20);
    expect(peakCol.peakCell).toBeGreaterThanOrEqual(peakCol.activeCells);
    expect(peakCol.peakHoldMs).toBeGreaterThan(0);
    expect(neighborCol.level).toBeGreaterThan(baseCol.level);
    expect(neighborCol.level).toBeLessThan(peakCol.level);
  });

  it("holds peak-hold blocks suspended at the apex while the spectrum drops, then pulls them down with gravity until idle", () => {
    const engine = new VoxelSpectrumEngine(16, 24);
    engine.pushSpectrum(new Array(32).fill(1.0));

    // Warm up attack so bars and peaks reach the top (row 24)
    engine.step(80);
    engine.step(80);
    const apexCell = engine.columns[0].peakCell;
    expect(apexCell).toBe(24);

    // Silence arrives: bars begin falling immediately, while peak-hold blocks remain at the apex during the hold window
    engine.clearTargets();
    engine.step(100);
    const midHoldCol = engine.columns[0];
    expect(midHoldCol.activeCells).toBeLessThan(apexCell);
    expect(midHoldCol.peakCell).toBe(apexCell);
    expect(midHoldCol.peakHoldMs).toBeGreaterThan(0);

    // Advance past the 340ms hold timer in 100ms frame steps so gravity accelerates the block downward
    engine.step(100);
    engine.step(100);
    engine.step(100);
    engine.step(100);
    engine.step(100);
    const fallingCol = engine.columns[0];
    expect(fallingCol.peakHoldMs).toBe(0);
    expect(fallingCol.peakVelocity).toBeGreaterThan(0);
    expect(fallingCol.peakCell).toBeLessThan(apexCell);
    expect(fallingCol.peakCell).toBeGreaterThan(0);

    // If a new transient exceeds the falling peak mid-fall, it pushes the peak back up and resets velocity
    const midFallLevel = fallingCol.peakLevel;
    engine.pushSpectrum(new Array(32).fill(1.0));
    engine.step(90);
    expect(engine.columns[0].peakLevel).toBeGreaterThan(midFallLevel);
    expect(engine.columns[0].peakVelocity).toBe(0);
    expect(engine.columns[0].peakHoldMs).toBeGreaterThan(0);

    // Finally, let everything decay to rest and verify step() reports false (auto-sleep)
    engine.clearTargets();
    let stillMoving = true;
    for (let i = 0; i < 60 && stillMoving; i++) {
      stillMoving = engine.step(50);
    }
    expect(stillMoving).toBe(false);
    expect(engine.columns.every((c) => c.activeCells === 0 && c.peakCell === 0)).toBe(true);
  });

  it("uses longer peak hold and gentler gravity under prefers-reduced-motion", () => {
    const normal = new VoxelSpectrumEngine(8, 24);
    const reduced = new VoxelSpectrumEngine(8, 24);
    const burst = new Array(32).fill(1.0);

    normal.pushSpectrum(burst);
    reduced.pushSpectrum(burst);
    normal.step(100, false);
    reduced.step(100, true);

    expect(reduced.columns[0].peakHoldMs).toBeGreaterThan(normal.columns[0].peakHoldMs);

    normal.clearTargets();
    reduced.clearTargets();
    // Advance 400ms: normal hold (340ms) has expired, reduced hold (480ms) is still holding
    for (let i = 0; i < 4; i++) {
      normal.step(100, false);
      reduced.step(100, true);
    }
    expect(normal.columns[0].peakHoldMs).toBe(0);
    expect(reduced.columns[0].peakHoldMs).toBeGreaterThan(0);
  });

  it("builds WCAG-compliant voxel palettes for both dark and light themes", () => {
    const darkPalette = buildVoxelThemePalette(LUMINOUS_DARK_COLORS, 28);
    const lightPalette = buildVoxelThemePalette(LUMINOUS_LIGHT_COLORS, 28);

    expect(darkPalette.isLight).toBe(false);
    expect(lightPalette.isLight).toBe(true);
    expect(darkPalette.rowColors).toHaveLength(28);
    expect(lightPalette.rowColors).toHaveLength(28);

    // Peak cap must meet >= 4.5:1 contrast against bg-main on both dark and light themes
    expect(
      calculateContrastRatio(darkPalette.peakCapColor, LUMINOUS_DARK_COLORS["bg-main"]),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      calculateContrastRatio(lightPalette.peakCapColor, LUMINOUS_LIGHT_COLORS["bg-main"]),
    ).toBeGreaterThanOrEqual(4.5);
  });

  it("renders the voxel matrix, reflection floor, and static dock preview onto a 2D canvas context", () => {
    const engine = new VoxelSpectrumEngine(16, 12);
    engine.pushSpectrum(new Array(32).fill(0.75));
    engine.step(60);

    const palette = buildVoxelThemePalette(LUMINOUS_DARK_COLORS, 12);
    const ctx = {
      clearRect: vi.fn(),
      fillRect: vi.fn(),
      fillStyle: "",
    } as unknown as CanvasRenderingContext2D;

    renderVoxelMatrix(ctx, 640, 320, engine, palette, { reducedMotion: false });
    const fullMotionRects = vi.mocked(ctx.fillRect).mock.calls.length;
    expect(ctx.clearRect).toHaveBeenCalledWith(0, 0, 640, 320);
    expect(fullMotionRects).toBeGreaterThan(16 * 12); // grid + reflection + separator

    vi.mocked(ctx.fillRect).mockClear();
    renderVoxelMatrix(ctx, 640, 320, engine, palette, { reducedMotion: true });
    const reducedMotionRects = vi.mocked(ctx.fillRect).mock.calls.length;
    // Reduced motion omits the reflection floor and baseline separator
    expect(reducedMotionRects).toBeLessThan(fullMotionRects);

    vi.mocked(ctx.fillRect).mockClear();
    renderStaticVoxelPreview(ctx, 96, 28, palette);
    expect(vi.mocked(ctx.fillRect).mock.calls.length).toBeGreaterThan(16);
  });
});

describe("voxel beat flash (#2246)", () => {
  it("flashes on an onset, fades within a second, then lets the engine rest", () => {
    const engine = new VoxelSpectrumEngine(8, 10);
    engine.pushOnset(0.8);
    expect(engine.beat).toBeCloseTo(0.8);
    // A weaker onset during the flash doesn't dim it.
    engine.pushOnset(0.3);
    expect(engine.beat).toBeCloseTo(0.8);

    expect(engine.step(16)).toBe(true);
    let moving = true;
    for (let i = 0; i < 60 && moving; i++) moving = engine.step(16);
    expect(engine.beat).toBe(0);
    expect(moving).toBe(false);
  });

  it("fades the same way under reduced motion, since it is brightness only", () => {
    const normal = new VoxelSpectrumEngine(8, 10);
    const reduced = new VoxelSpectrumEngine(8, 10);
    normal.pushOnset(1);
    reduced.pushOnset(1);
    normal.step(50, false);
    reduced.step(50, true);
    expect(reduced.beat).toBeCloseTo(normal.beat);
  });

  it("draws the flash over the bass rows of lit bars and restores opacity", () => {
    const engine = new VoxelSpectrumEngine(4, 10);
    engine.pushSpectrum([1, 1, 1, 1]);
    for (let i = 0; i < 20; i++) engine.step(16);
    const palette = buildVoxelThemePalette(LUMINOUS_DARK_COLORS, 10);
    const alphas: number[] = [];
    const ctx = {
      clearRect: vi.fn(),
      fillRect: vi.fn(function (this: CanvasRenderingContext2D) {
        alphas.push(this.globalAlpha);
      }),
      fillStyle: "",
      globalAlpha: 1,
    } as unknown as CanvasRenderingContext2D;

    renderVoxelMatrix(ctx, 200, 200, engine, palette, { reducedMotion: true });
    expect(alphas.every((a) => a === 1)).toBe(true);

    alphas.length = 0;
    engine.pushOnset(1);
    renderVoxelMatrix(ctx, 200, 200, engine, palette, { reducedMotion: true });
    // 3 bass rows (30% of 10) × 4 lit columns drawn translucent.
    expect(alphas.filter((a) => a < 1)).toHaveLength(12);
    expect(ctx.globalAlpha).toBe(1);
  });
});

describe("voxel preset (#2245)", () => {
  function fakeCtx() {
    return {
      clearRect: vi.fn(),
      fillRect: vi.fn(),
      beginPath: vi.fn(),
      roundRect: vi.fn(),
      fill: vi.fn(),
      createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
      fillStyle: "",
    } as unknown as CanvasRenderingContext2D;
  }

  it("draws the mini slot once per frame of data, with no animation of its own", () => {
    const mini = VOXEL_PRESET.create("mini");
    mini.setTheme(LUMINOUS_DARK_COLORS);
    mini.push({ bands: new Array(32).fill(0.5), playing: true, onset: 0, scope: null });
    expect(mini.step(16, false)).toBe(false);

    const ctx = fakeCtx();
    mini.render(ctx, 96, 28, { reducedMotion: false, preview: false });
    expect(ctx.roundRect).toHaveBeenCalledTimes(32);
    mini.render(ctx, 96, 28, { reducedMotion: false, preview: false });
    // The bar gradient is built once per theme and height, not per draw.
    expect(ctx.createLinearGradient).toHaveBeenCalledTimes(1);
  });

  it("settles the full matrix to rest when playback stops", () => {
    const full = VOXEL_PRESET.create("full");
    full.setTheme(LUMINOUS_DARK_COLORS);
    full.push({ bands: new Array(32).fill(0.9), playing: true, onset: 0, scope: null });
    expect(full.step(16, false)).toBe(true);

    full.push({ bands: new Array(32).fill(0.9), playing: false, onset: 0, scope: null });
    let moving = true;
    for (let i = 0; i < 500 && moving; i++) moving = full.step(16, false);
    expect(moving).toBe(false);
  });

  it("sweeps across the matrix on triggerSweep and cancels under reduced motion", () => {
    const engine = new VoxelSpectrumEngine(16, 12);
    engine.triggerSweep();
    expect(engine.sweep).toBeGreaterThan(0);
    expect(engine.step(100, false)).toBe(true);
    expect(engine.step(100, false)).toBe(true);
    expect(engine.sweep).toBeGreaterThan(0.4);

    // Finishes on its own
    for (let i = 0; i < 20; i++) engine.step(50, false);
    expect(engine.sweep).toBe(0);

    // Cancelled immediately under reduced motion
    engine.triggerSweep();
    engine.step(16, true);
    expect(engine.sweep).toBe(0);
  });

  it("blends valid trackAccent into crest colors and exposes bloomColor", () => {
    const base = buildVoxelThemePalette(LUMINOUS_DARK_COLORS, 12);
    const tinted = buildVoxelThemePalette(LUMINOUS_DARK_COLORS, 12, "#ff5500");
    expect(base.bloomColor).toMatch(/^rgba\(/);
    expect(tinted.rowColors[11]).not.toBe(base.rowColors[11]);
    expect(tinted.rowColors[0]).toBe(base.rowColors[0]);
  });
});
