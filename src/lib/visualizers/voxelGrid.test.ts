import { describe, expect, it, vi } from "vitest";
import { LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "../stores/theme.svelte";
import {
  VoxelCellPainter,
  VoxelGrid,
  VoxelLayerCache,
  voxelAlphaLadder,
  voxelUnlitCellColor,
  type Ctx2D,
} from "./voxelGrid";

/** A 2D context that records each filled rect with the fill it was drawn in. */
function recordingContext() {
  const rects: { x: number; y: number; w: number; h: number; style: string }[] = [];
  const styleChanges: string[] = [];
  let fillStyle = "";
  const ctx = {
    get fillStyle() {
      return fillStyle;
    },
    set fillStyle(v: string) {
      fillStyle = v;
      styleChanges.push(v);
    },
    fillRect(x: number, y: number, w: number, h: number) {
      rects.push({ x, y, w, h, style: fillStyle });
    },
    clearRect() {},
  };
  return { ctx: ctx as unknown as CanvasRenderingContext2D, rects, styleChanges };
}

describe("VoxelGrid", () => {
  it("fits square cells at the largest whole pitch, centred, with an LED gap", () => {
    const grid = new VoxelGrid();
    expect(grid.fit(500, 210, 10, 4)).toBe(true);
    expect(grid.pitch).toBe(50);
    expect(grid.cell).toBe(41);
    expect(grid.originX).toBe(0);
    expect(grid.originY).toBe(5);
    expect(grid.fit(500, 210, 10, 4)).toBe(false);
  });

  it("keeps a 1 px gap at small pitches and none at 1 px", () => {
    const grid = new VoxelGrid();
    grid.fit(30, 9, 10, 3);
    expect([grid.pitch, grid.cell]).toEqual([3, 2]);
    grid.fit(10, 3, 10, 3);
    expect([grid.pitch, grid.cell]).toEqual([1, 1]);
  });

  it("spans the whole canvas around the grid, past its edges", () => {
    const grid = new VoxelGrid();
    grid.fit(100, 40, 4, 4);
    const out = new Int32Array(4);
    grid.span(100, 40, out);
    expect(Array.from(out)).toEqual([-3, 0, 7, 4]);
  });
});

describe("VoxelCellPainter", () => {
  it("draws each non-empty cell in its fill, setting each fill once", () => {
    const grid = new VoxelGrid();
    grid.fit(30, 20, 3, 2);
    const styles = new Uint8Array([2, 0, 1, 1, 2, 2]);
    const { ctx, rects, styleChanges } = recordingContext();
    new VoxelCellPainter().paint(ctx, grid, styles, ["none", "red", "blue"]);
    expect(styleChanges).toEqual(["red", "blue"]);
    expect(rects.map((r) => [r.x, r.y, r.style])).toEqual([
      [20, 0, "red"],
      [0, 10, "red"],
      [0, 0, "blue"],
      [10, 10, "blue"],
      [20, 10, "blue"],
    ]);
  });
});

describe("VoxelLayerCache", () => {
  function blittingContext() {
    const { ctx } = recordingContext();
    const blits: unknown[] = [];
    (ctx as unknown as { drawImage: (...a: unknown[]) => void }).drawImage = (...a) =>
      void blits.push(a);
    return { ctx, blits };
  }

  it("paints once and blits every frame until stale or resized", () => {
    vi.stubGlobal("document", undefined);
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        constructor(
          public width: number,
          public height: number,
        ) {}
        getContext() {
          return { setTransform() {}, clearRect() {}, fillRect() {} };
        }
      },
    );
    try {
      const cache = new VoxelLayerCache();
      const paint = vi.fn((_c: Ctx2D) => {});
      const { ctx, blits } = blittingContext();
      cache.blit(ctx, 100, 50, false, paint);
      cache.blit(ctx, 100, 50, false, paint);
      expect(paint).toHaveBeenCalledTimes(1);
      expect(blits).toHaveLength(2);
      cache.blit(ctx, 100, 50, true, paint);
      cache.blit(ctx, 120, 50, false, paint);
      expect(paint).toHaveBeenCalledTimes(3);
      expect(blits).toHaveLength(4);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("paints straight onto a context that can't blit", () => {
    const { ctx } = recordingContext();
    const paint = vi.fn((_c: Ctx2D) => {});
    new VoxelLayerCache().blit(ctx, 100, 50, false, paint);
    expect(paint).toHaveBeenCalledWith(ctx);
  });
});

describe("voxel theme fills", () => {
  it("keeps the shared unlit cell colour the presets have always drawn", () => {
    expect(voxelUnlitCellColor(LUMINOUS_DARK_COLORS)).toMatch(/^rgba\(\d+, \d+, \d+, 0\.1\)$/);
    expect(voxelUnlitCellColor(LUMINOUS_LIGHT_COLORS)).toMatch(/^rgba\(\d+, \d+, \d+, 0\.13\)$/);
    expect(voxelUnlitCellColor(LUMINOUS_DARK_COLORS, 2)).toMatch(/, 0\.2\)$/);
  });

  it("steps a ladder from its floor opacity to full", () => {
    expect(voxelAlphaLadder("#ff0000", 3, 0.2)).toEqual([
      "rgba(255, 0, 0, 0.2)",
      "rgba(255, 0, 0, 0.6)",
      "rgba(255, 0, 0, 1)",
    ]);
    expect(voxelAlphaLadder("#ff0000", 2, 0.1, 0.5)).toEqual([
      "rgba(255, 0, 0, 0.1)",
      "rgba(255, 0, 0, 0.5)",
    ]);
  });
});
