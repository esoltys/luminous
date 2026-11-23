import { describe, expect, it } from "vitest";
import { FACE_ALL, FACE_LEFT, FACE_RIGHT, FACE_TOP, IsoView, isoFaces } from "./isoVoxel";
import type { Ctx2D } from "./voxelGrid";

function recordingPolyContext() {
  const fills: { style: string; points: [number, number][] }[] = [];
  let currentPoints: [number, number][] = [];
  let fillStyle = "";
  const ctx = {
    get fillStyle() {
      return fillStyle;
    },
    set fillStyle(v: string) {
      fillStyle = v;
    },
    beginPath() {
      currentPoints = [];
    },
    moveTo(x: number, y: number) {
      currentPoints.push([x, y]);
    },
    lineTo(x: number, y: number) {
      currentPoints.push([x, y]);
    },
    closePath() {},
    fill() {
      fills.push({ style: fillStyle, points: currentPoints });
    },
  };
  return { ctx: ctx as unknown as Ctx2D, fills };
}

describe("isoFaces", () => {
  it("keeps top in the given hex and darkens left and right faces progressively", () => {
    const faces = isoFaces("#6496c8");
    expect(faces.top).toBe("#6496c8");
    expect(faces.left).toBe("#5078a0");
    expect(faces.right).toBe("#406080");
  });
});

describe("IsoView", () => {
  it("fits a 3D box into a canvas and reports whether projection changed", () => {
    const view = new IsoView();
    expect(view.fit(400, 300, 0, 10, 0, 10, 0, 5)).toBe(true);
    expect(view.unit).toBeGreaterThan(0.5);
    expect(view.fit(400, 300, 0, 10, 0, 10, 0, 5)).toBe(false);
    expect(view.fit(800, 600, 0, 10, 0, 10, 0, 5)).toBe(true);
  });

  it("ignores degenerate canvas or bounding-box dimensions without producing NaN", () => {
    const view = new IsoView();
    view.fit(400, 300, 0, 4, 0, 4, 0, 4);
    const snapshot = [view.unit, view.originX, view.originY];

    expect(view.fit(0, 300, 0, 4, 0, 4, 0, 4)).toBe(false);
    expect(view.fit(400, -10, 0, 4, 0, 4, 0, 4)).toBe(false);
    expect(view.fit(NaN, 300, 0, 4, 0, 4, 0, 4)).toBe(false);
    expect(view.fit(400, 300, 2, 2, 2, 2, 0, 4)).toBe(false);
    expect([view.unit, view.originX, view.originY]).toEqual(snapshot);
  });

  it("projects scene coordinates in 2:1 isometric axes", () => {
    const view = new IsoView();
    view.unit = 10;
    view.originX = 100;
    view.originY = 50;
    expect(view.screenX(2, 1)).toBe(110);
    expect(view.screenY(2, 1, 1)).toBe(55);
  });

  it("draws only the faces included in the mask", () => {
    const view = new IsoView();
    const faces = isoFaces("#80a0c0");

    const all = recordingPolyContext();
    view.cube(all.ctx, 0, 0, 0, 1, faces, FACE_ALL);
    expect(all.fills.map((f) => f.style)).toEqual([faces.top, faces.left, faces.right]);

    const topOnly = recordingPolyContext();
    view.cube(topOnly.ctx, 0, 0, 0, 1, faces, FACE_TOP);
    expect(topOnly.fills.map((f) => f.style)).toEqual([faces.top]);

    const sides = recordingPolyContext();
    view.cube(sides.ctx, 0, 0, 0, 1, faces, FACE_LEFT | FACE_RIGHT);
    expect(sides.fills.map((f) => f.style)).toEqual([faces.left, faces.right]);
  });

  it("draws plane without gap and tile with gap", () => {
    const view = new IsoView();
    view.unit = 10;
    const { ctx, fills } = recordingPolyContext();
    view.plane(ctx, 0, 0, 1, 1, 0, "#111111");
    view.tile(ctx, 0, 0, 0, 1, "#222222");

    expect(fills).toHaveLength(2);
    // Plane corners span the full [0, 1] x [0, 1] cell; tile is inset by GAP.
    expect(fills[0].points[0]).toEqual([0, 0]);
    expect(fills[1].points[0][0]).toBe(0);
    expect(fills[1].points[0][1]).toBeGreaterThan(0);
  });
});
