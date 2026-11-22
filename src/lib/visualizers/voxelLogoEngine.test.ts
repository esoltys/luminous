import { describe, expect, it, vi } from "vitest";
import { LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "../stores/theme.svelte";
import { calculateContrastRatio, rgbToHex } from "../utils/colorUtils";
import type { VisualizerFrame } from "./types";
import { VOXEL_LOGO_PRESET } from "./voxelLogo";
import {
  LOGO_REST,
  LOGO_STYLE,
  LogoGrid,
  LogoLevels,
  VoxelLogoRenderer,
  buildVoxelLogoPalette,
  type VoxelLogoPalette,
} from "./voxelLogoEngine";

interface Rect {
  style: string;
  x: number;
  y: number;
}

/** A 2D context that records each filled cell with the fill it was drawn in. */
function recordingContext() {
  const rects: Rect[] = [];
  const ctx = {
    fillStyle: "",
    /** Removes the cells drawn inside the cleared rectangle. */
    clearRect(x: number, y: number, w: number, h: number) {
      const kept = rects.filter((r) => r.x < x || r.x >= x + w || r.y < y || r.y >= y + h);
      rects.length = 0;
      rects.push(...kept);
    },
    fillRect(x: number, y: number) {
      rects.push({ style: ctx.fillStyle, x, y });
    },
  };
  return { ctx: ctx as unknown as CanvasRenderingContext2D, rects };
}

function frame(bands: number[], onset = 0, playing = true): VisualizerFrame {
  return { bands, playing, onset, scope: null };
}

/** 32 bands with `value` in [from, to) and silence elsewhere. */
function bandsIn(from: number, to: number, value = 1): number[] {
  return Array.from({ length: 32 }, (_, i) => (i >= from && i < to ? value : 0));
}

/** Index of the cell under mark coordinates (x, y). */
function cellAt(grid: LogoGrid, min: number, x: number, y: number): number {
  const col = Math.floor((x - min) / grid.cellUnits);
  const row = Math.floor((y - min) / grid.cellUnits);
  return row * grid.size + col;
}

type Layer = "rim" | "ring" | "glow" | "burst";

/** A palette whose fills are their own style indices, so a drawn cell says which layer it is. */
const TAGGED: VoxelLogoPalette = {
  styles: Array.from({ length: LOGO_STYLE.count }, (_, i) => String(i)),
};

/** Positions of the cells drawn in `layer`'s ladder. */
function cellsIn(rects: Rect[], layer: Layer): Set<string> {
  const from = LOGO_STYLE[layer];
  return new Set(
    rects
      .filter((r) => Number(r.style) >= from && Number(r.style) < from + LOGO_STYLE.ladder)
      .map((r) => `${r.x},${r.y}`),
  );
}

describe("LogoGrid (#2252)", () => {
  it.each([
    [56, -40, 240],
    [13, -4, 204],
    [11, -4, 204],
    [9, -4, 204],
  ])("keeps the disc, rim and ring as closed shapes at %i cells", (size, min, max) => {
    const grid = new LogoGrid(size, min, max);
    expect(grid.restLayer[cellAt(grid, min, 100, 100)]).toBe(LOGO_REST.disc);

    // Walking out from the centre along each axis crosses disc, then rim, then ring.
    for (const [dx, dy] of [
      [1, 0],
      [-1, 0],
      [0, 1],
      [0, -1],
    ]) {
      const seen: number[] = [];
      for (let t = 0; t <= 100; t += grid.cellUnits / 4) {
        const layer = grid.restLayer[cellAt(grid, min, 100 + dx * t, 100 + dy * t)];
        if (layer !== seen[seen.length - 1]) seen.push(layer);
      }
      expect(seen.filter((l) => l !== LOGO_REST.none)).toEqual([
        LOGO_REST.disc,
        LOGO_REST.rim,
        LOGO_REST.ring,
      ]);
    }
  });

  it("stores each cell's distance from the burst's centre", () => {
    const grid = new LogoGrid(56, -40, 240);
    const i = cellAt(grid, -40, 150.6, 59.6);
    expect(grid.burstDistance[i]).toBeLessThan(grid.cellUnits);
  });
});

describe("LogoLevels (#2252)", () => {
  it.each([
    ["bass", 0, 8],
    ["mid", 8, 20],
    ["treble", 20, 32],
  ] as const)("drives only %s from its band range", (layer, from, to) => {
    const levels = new LogoLevels();
    levels.push(frame(bandsIn(from, to)));
    levels.settle();
    for (const other of ["bass", "mid", "treble"] as const) {
      if (other === layer) expect(levels[other]).toBe(1);
      else expect(levels[other]).toBe(0);
    }
  });

  it("flares the burst on a beat onset, then decays it with the bass", () => {
    const levels = new LogoLevels();
    levels.push(frame(bandsIn(0, 0), 0.9));
    levels.step(16, false);
    const flared = levels.bass;
    expect(flared).toBeGreaterThan(0.3);
    expect(levels.mid).toBe(0);

    levels.push(frame(bandsIn(0, 0)));
    levels.step(50, false);
    expect(levels.bass).toBeLessThan(flared);
  });

  it("settles to rest when playback stops and then reports nothing left to draw", () => {
    const levels = new LogoLevels();
    levels.push(frame(bandsIn(0, 32)));
    expect(levels.step(16, false)).toBe(true);

    levels.push(frame(bandsIn(0, 32), 0, false));
    let frames = 0;
    while (levels.step(16, false)) {
      frames++;
      expect(frames).toBeLessThan(200);
    }
    expect([levels.bass, levels.mid, levels.treble]).toEqual([0, 0, 0]);
  });

  it("decays more gently under reduced motion", () => {
    const normal = LogoLevels.fixed(1, 1, 1);
    const reduced = LogoLevels.fixed(1, 1, 1);
    normal.push(frame([], 0, false));
    reduced.push(frame([], 0, false));
    normal.step(50, false);
    reduced.step(50, true);
    expect(reduced.mid).toBeGreaterThan(normal.mid);
  });
});

describe("LogoLevels beat ripples (#2270)", () => {
  const RING_OUTER = 96;

  it("sends a ripple outward from the ring on each beat, fading until it is gone", () => {
    const levels = new LogoLevels();
    expect(levels.rippleAt(RING_OUTER, 1)).toBe(0);

    levels.push(frame(bandsIn(0, 0), 0.8));
    const start = levels.rippleAt(RING_OUTER, 1);
    expect(start).toBeGreaterThan(0);

    // 100 ms at 150 units/s carries it 15 units out, dimmer than it started.
    expect(levels.step(100, false)).toBe(true);
    expect(levels.rippleAt(RING_OUTER, 1)).toBe(0);
    const later = levels.rippleAt(RING_OUTER + 15, 1);
    expect(later).toBeGreaterThan(0);
    expect(later).toBeLessThan(start);
    expect(levels.rippling).toBe(true);

    let frames = 0;
    while (levels.step(16, false)) {
      frames++;
      expect(frames).toBeLessThan(200);
    }
    expect(levels.rippling).toBe(false);
    for (let d = 0; d <= 400; d += 2) expect(levels.rippleAt(d, 1)).toBe(0);
  });

  it("keeps animating while a ripple is in flight, even once the levels have arrived", () => {
    // Full bass already settled, so the beat leaves every level where it is.
    const levels = new LogoLevels();
    levels.push(frame(bandsIn(0, 8)));
    levels.settle();
    levels.push(frame(bandsIn(0, 8), 1));
    let frames = 0;
    while (levels.step(16, false)) {
      frames++;
      expect(frames).toBeLessThan(200);
    }
    // 220 units of travel at 150 units/s.
    expect(frames * 16).toBeGreaterThan(1400);
    expect(levels.bass).toBe(1);
  });

  it("is still lit at a 16:9 stage's side edges, about 250 units out, and gone by 316", () => {
    const levels = new LogoLevels();
    levels.push(frame(bandsIn(0, 0), 1));
    let atSides = 0;
    // Probing 3 units either side, nothing past 319 means no ripple is alive beyond 316.
    do {
      for (let d = 319.5; d <= 400; d += 1) expect(levels.rippleAt(d, 3)).toBe(0);
      atSides = Math.max(atSides, levels.rippleAt(250, 3));
    } while (levels.step(16, false));
    expect(atSides).toBeGreaterThan(0.25);
  });

  it("starts stronger beats brighter", () => {
    const soft = new LogoLevels();
    const hard = new LogoLevels();
    soft.push(frame(bandsIn(0, 0), 0.1));
    hard.push(frame(bandsIn(0, 0), 1));
    expect(hard.rippleAt(RING_OUTER, 1)).toBeGreaterThan(soft.rippleAt(RING_OUTER, 1));
    expect(hard.rippleAt(RING_OUTER, 1)).toBeLessThanOrEqual(1);
  });

  it("replaces the oldest ripple when a seventh beat arrives with six in flight", () => {
    const levels = new LogoLevels();
    for (let beat = 0; beat < 7; beat++) {
      levels.push(frame(bandsIn(0, 0), 1));
      if (beat < 6) levels.step(20, false);
    }
    // 20 ms steps move each ripple 3 units: the first would now be at 114, the next at 111.
    expect(levels.rippleAt(114, 1)).toBe(0);
    for (let r = RING_OUTER; r <= 111; r += 3) {
      expect(levels.rippleAt(r, 1)).toBeGreaterThan(0);
    }
  });

  it("drops ripples under reduced motion and on settle()", () => {
    const reduced = new LogoLevels();
    reduced.push(frame(bandsIn(0, 0), 1));
    reduced.step(16, true);
    for (let d = 0; d <= 240; d += 2) expect(reduced.rippleAt(d, 1)).toBe(0);

    const settled = new LogoLevels();
    settled.push(frame(bandsIn(0, 0), 1));
    settled.settle();
    expect(settled.rippleAt(RING_OUTER, 1)).toBe(0);
    expect(settled.step(16, false)).toBe(false);
  });
});

describe("VoxelLogoRenderer (#2252)", () => {
  const palette = buildVoxelLogoPalette(LUMINOUS_DARK_COLORS, "bg-main");

  function fullRenderer() {
    return new VoxelLogoRenderer({
      grids: [new LogoGrid(56, -40, 240)],
      housing: true,
      glow: true,
      ripples: true,
    });
  }

  it("picks the finest mini grid that keeps cells at least 3 px, else 2 px", () => {
    const renderer = new VoxelLogoRenderer({
      grids: [13, 11, 9].map((n) => new LogoGrid(n, -4, 204)),
      housing: false,
      glow: false,
      ripples: false,
    });
    expect(renderer.gridFor(96, 40).size).toBe(13);
    expect(renderer.gridFor(96, 28).size).toBe(9);
    expect(renderer.gridFor(64, 20).size).toBe(9);
    expect(renderer.gridFor(96, 33).size).toBe(11);
    expect(renderer.gridFor(10, 10).size).toBe(9);
  });

  it("draws only fills from the palette, on whole-pixel cells", () => {
    const { ctx, rects } = recordingContext();
    fullRenderer().render(ctx, 800, 450, LogoLevels.fixed(0.8, 0.6, 0.7), palette, false);
    expect(rects.length).toBeGreaterThan(56 * 56);
    const known = new Set(palette.styles);
    for (const r of rects) {
      expect(known.has(r.style)).toBe(true);
      expect(Number.isInteger(r.x) && Number.isInteger(r.y)).toBe(true);
    }
  });

  /** Cells drawn at `level` on every band, with the tagged palette. */
  function draw(level: number, reducedMotion: boolean): Rect[] {
    const { ctx, rects } = recordingContext();
    fullRenderer().render(
      ctx,
      560,
      560,
      LogoLevels.fixed(level, level, level),
      TAGGED,
      reducedMotion,
    );
    return rects;
  }

  it("grows the burst halo, thickens the ring and widens the glow with the music", () => {
    const quiet = draw(0, false);
    const loud = draw(1, false);
    expect(cellsIn(loud, "burst").size).toBeGreaterThan(cellsIn(quiet, "burst").size + 20);
    expect(cellsIn(loud, "ring").size).toBeGreaterThan(cellsIn(quiet, "ring").size);
    expect(cellsIn(loud, "glow").size).toBeGreaterThan(cellsIn(quiet, "glow").size);
  });

  it("holds the geometry still under reduced motion, changing only brightness", () => {
    const quiet = draw(0, true);
    const loud = draw(1, true);
    // Rim and ring keep their cells; only the burst halo, painted on top, may brighten over some.
    const loudHalo = cellsIn(loud, "burst");
    for (const layer of ["rim", "ring"] as const) {
      const before = [...cellsIn(quiet, layer)].filter((c) => !loudHalo.has(c));
      expect(new Set(before)).toEqual(cellsIn(loud, layer));
    }
    // The halo and glow fade in within a fixed reach rather than growing to their full size.
    expect(cellsIn(loud, "burst").size).toBeLessThan(cellsIn(draw(1, false), "burst").size);
    const fills = (rects: Rect[], layer: Layer) =>
      new Set(rects.filter((r) => cellsIn([r], layer).size > 0).map((r) => r.style));
    expect(fills(loud, "rim")).not.toEqual(fills(quiet, "rim"));
    expect(fills(loud, "ring")).not.toEqual(fills(quiet, "ring"));
  });

  describe("beat ripples (#2270)", () => {
    /**
     * Mark-unit distance from the centre of the cell drawn at (x, y) on a canvas 560 px tall and
     * `width` wide: 10 px pitch, the 56-cell grid centred horizontally.
     */
    function markDistanceOf(r: Rect, width = 560): number {
      const cellUnits = 280 / 56;
      const originX = (width - 560) / 2;
      const mx = -40 + ((r.x - originX) / 10 + 0.5) * cellUnits;
      const my = -40 + (r.y / 10 + 0.5) * cellUnits;
      return Math.hypot(mx - 100, my - 100);
    }

    /** Levels with one strong ripple `ms` out (at 150 units/s from radius 96). */
    function rippling(ms = 200): LogoLevels {
      const levels = new LogoLevels();
      levels.push(frame(bandsIn(0, 32, 0.5), 1));
      for (let t = 0; t < ms; t += 100) levels.step(100, false);
      return levels;
    }

    function drawWith(
      ripples: boolean,
      levels: LogoLevels,
      reducedMotion = false,
      width = 560,
    ): Rect[] {
      const { ctx, rects } = recordingContext();
      new VoxelLogoRenderer({
        grids: [new LogoGrid(56, -40, 240)],
        housing: true,
        glow: true,
        ripples,
      }).render(ctx, width, 560, levels, TAGGED, reducedMotion);
      return rects;
    }

    /** Cells whose fill differs between two renders of the same grid. */
    function changed(a: Rect[], b: Rect[]): { before: Rect; after: Rect }[] {
      const byPos = new Map(a.map((r) => [`${r.x},${r.y}`, r]));
      return b
        .filter((r) => byPos.get(`${r.x},${r.y}`)?.style !== r.style)
        .map((after) => ({ before: byPos.get(`${after.x},${after.y}`)!, after }));
    }

    it("lights a ring of cells at the ripple's radius in the ring's colour", () => {
      const levels = rippling();
      const lit = changed(drawWith(false, levels), drawWith(true, levels));
      expect(lit.length).toBeGreaterThan(40);
      for (const { after } of lit) {
        expect(cellsIn([after], "ring").size).toBe(1);
        // The band is 3 units either side of the radius, measured at cell centres.
        expect(Math.abs(markDistanceOf(after) - 126)).toBeLessThanOrEqual(3.01);
      }
    });

    it("carries on past the grid across a wide stage's housing, out to the left and right", () => {
      // 700 ms out: radius 201, past the grid's sides (140 units from the centre).
      const levels = rippling(700);
      const width = 1120;
      const lit = changed(
        drawWith(false, levels, false, width),
        drawWith(true, levels, false, width),
      );
      const outside = lit.filter(({ after }) => after.x < 280 || after.x >= 840);
      expect(outside.filter(({ after }) => after.x < 280).length).toBeGreaterThan(10);
      expect(outside.filter(({ after }) => after.x >= 840).length).toBeGreaterThan(10);
      for (const { after } of lit) {
        expect(cellsIn([after], "ring").size).toBe(1);
        expect(Math.abs(markDistanceOf(after, width) - 201)).toBeLessThanOrEqual(3.01);
      }
    });

    it("only replaces housing and glow cells, never the mark's own layers", () => {
      const levels = rippling();
      const housing = String(LOGO_STYLE.housing);
      for (const { before } of changed(drawWith(false, levels), drawWith(true, levels))) {
        expect(before.style === housing || cellsIn([before], "glow").size === 1).toBe(true);
      }
    });

    it("draws no ripples under reduced motion", () => {
      const levels = rippling();
      expect(changed(drawWith(false, levels, true), drawWith(true, levels, true))).toHaveLength(0);
    });
  });

  it("skips outer housing tiling when pitch drops below 3 px and ignores invalid sizes (#2262)", () => {
    const renderer = fullRenderer();
    const { ctx, rects } = recordingContext();
    renderer.render(ctx, 1920, 60, LogoLevels.fixed(0.5, 0.5, 0.5), palette, false);
    expect(rects.length).toBeLessThanOrEqual(56 * 56);

    rects.length = 0;
    for (const [w, h] of [
      [0, 100],
      [100, 0],
      [-50, 100],
      [Number.NaN, 100],
    ]) {
      renderer.render(ctx, w, h, LogoLevels.fixed(0.5, 0.5, 0.5), palette, false);
      expect(rects).toHaveLength(0);
    }
  });

  it("never produces NaN styles when mid is near zero (#2262)", () => {
    const renderer = fullRenderer();
    const { ctx, rects } = recordingContext();
    for (const mid of [0, 1 / 35, 0.01]) {
      renderer.render(ctx, 560, 560, LogoLevels.fixed(0, mid, 0), TAGGED, false);
      for (const r of rects) {
        expect(Number.isNaN(Number(r.style))).toBe(false);
      }
    }
  });

  it("caches outer housing offscreen and blits via drawImage across frames (#2262)", () => {
    let offscreenFillCount = 0;
    const origOffscreen = globalThis.OffscreenCanvas;
    const origDoc = globalThis.document;
    // Force OffscreenCanvas path by stubbing document.createElement to return null context
    // and providing a minimal OffscreenCanvas mock.
    class FakeOffscreenCanvas {
      width: number;
      height: number;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
      }
      getContext() {
        return {
          fillStyle: "",
          setTransform() {},
          clearRect() {},
          fillRect() {
            offscreenFillCount++;
          },
        };
      }
    }
    vi.stubGlobal("OffscreenCanvas", FakeOffscreenCanvas);
    vi.stubGlobal("document", undefined);

    try {
      const renderer = fullRenderer();
      const drawImages: unknown[] = [];
      const { ctx, rects } = recordingContext();
      (ctx as unknown as { drawImage: (...args: unknown[]) => void }).drawImage = (...args) => {
        drawImages.push(args);
      };
      (ctx as unknown as { getTransform: () => { a: number } }).getTransform = () => ({ a: 1 });

      renderer.render(ctx, 1920, 1080, LogoLevels.fixed(0.5, 0.5, 0.5), palette, false);
      expect(drawImages).toHaveLength(1);
      expect(rects.length).toBeLessThanOrEqual(56 * 56);
      const firstOffscreenFills = offscreenFillCount;
      expect(firstOffscreenFills).toBeGreaterThan(0);

      // Second frame at same size & theme: reuses cached offscreen housing without re-filling it.
      renderer.render(ctx, 1920, 1080, LogoLevels.fixed(0.8, 0.2, 0.9), palette, false);
      expect(drawImages).toHaveLength(2);
      expect(offscreenFillCount).toBe(firstOffscreenFills);

      // Resize invalidates the cache and rebuilds once.
      renderer.render(ctx, 1280, 720, LogoLevels.fixed(0.8, 0.2, 0.9), palette, false);
      expect(drawImages).toHaveLength(3);
      expect(offscreenFillCount).toBeGreaterThan(firstOffscreenFills);
    } finally {
      vi.stubGlobal("OffscreenCanvas", origOffscreen);
      vi.stubGlobal("document", origDoc);
    }
  });
});

describe("buildVoxelLogoPalette (#2252)", () => {
  function topRgbHex(style: string): string {
    const [r, g, b] = style
      .slice(5)
      .split(",")
      .map((v) => parseFloat(v));
    return rgbToHex(r, g, b);
  }

  it.each([
    ["dark", LUMINOUS_DARK_COLORS],
    ["light", LUMINOUS_LIGHT_COLORS],
  ] as const)("keeps the burst visible against the %s theme's surfaces", (_, colors) => {
    for (const surface of ["bg-main", "bg-playerbar"] as const) {
      const palette = buildVoxelLogoPalette(colors, surface);
      const burst = topRgbHex(palette.styles[LOGO_STYLE.burst + LOGO_STYLE.ladder - 1]);
      expect(calculateContrastRatio(burst, colors[surface])).toBeGreaterThanOrEqual(3);
    }
  });
});

describe("VOXEL_LOGO_PRESET (#2252)", () => {
  it("draws a still preview at the menu thumbnail and playbar slot sizes", () => {
    const mini = VOXEL_LOGO_PRESET.create("mini");
    mini.setTheme(LUMINOUS_DARK_COLORS);
    for (const [w, h] of [
      [64, 20],
      [96, 28],
    ]) {
      const { ctx, rects } = recordingContext();
      mini.render(ctx, w, h, { reducedMotion: false, preview: true });
      expect(rects.length).toBeGreaterThan(40);
      expect(rects.every((r) => r.y >= 0 && r.y < h)).toBe(true);
    }
  });

  it("keeps the mini view unsmoothed: it never asks for more frames", () => {
    const mini = VOXEL_LOGO_PRESET.create("mini");
    mini.setTheme(LUMINOUS_DARK_COLORS);
    mini.push(frame(bandsIn(0, 32)));
    expect(mini.step(16, false)).toBe(false);
  });

  it("animates the full view while playing and stops once paused and settled", () => {
    const full = VOXEL_LOGO_PRESET.create("full");
    full.setTheme(LUMINOUS_DARK_COLORS);
    full.push(frame(bandsIn(0, 32)));
    expect(full.step(16, false)).toBe(true);
    full.push(frame(bandsIn(0, 32), 0, false));
    let frames = 0;
    while (full.step(16, false)) frames++;
    expect(frames).toBeGreaterThan(0);
    expect(full.step(16, false)).toBe(false);
  });
});
