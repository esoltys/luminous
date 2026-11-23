import {
  BANK_WIDTH,
  type FishLadder,
  MAX_FISH,
  MAX_POOLS,
  NOTCH_WIDTH,
  MAX_RIPPLES,
  RIVER_WIDTH,
  WATER_DEPTH,
} from "./fishLadderEngine";
import { FACE_ALL, FACE_LEFT, FACE_RIGHT, FACE_TOP, IsoView, type IsoFaces } from "./isoVoxel";
import { VoxelLayerCache, type Ctx2D } from "./voxelGrid";
import type { SalmonRunPalette } from "./salmonRun";

// The fish ladder drawn as isometric voxels. The terrain (banks, stone, water and weirs) holds
// still for a whole track, so it is painted once onto a cached layer; everything that moves or
// flickers (falls, boulders, salmon, leaf, glints and ripples) is drawn over it each frame.
//
// That split is only correct because of a rule the ladder's layout keeps: nothing in the terrain
// stands taller than the terrain behind it as seen from the viewer. The pools step *down* toward
// the viewer, there is no near bank, and each pool's water is flush with its weir, so the terrain
// can never hide something drawn over it. Boulders and falls do stand up out of the water, which
// is why they're drawn with the moving things, in depth order, and not cached.

/** Brightness steps of foam (falls, ripples) and glints. */
export const FOAM_STEPS = 8;
/** Steps of the reduced-motion light shift above unlit, the last fully lit. */
export const LIGHT_STEPS = 6;
/** Edge of the leaf's and ripples' tiles, in terrain cubes: they're finer than the terrain. */
const FINE = 0.5;
/** Edge of a salmon's cubes before its own size multiplier: a salmon is seven of them long. */
const FISH_CUBE = 0.62;

/** Terrain cell kinds, indices into `terrainFaces`; 0 is empty. */
const T = {
  empty: 0,
  stone: 1,
  cap: 2,
  bankTop: 3,
  deep: 4,
  water: 5,
  spill: 6,
} as const;

// A salmon as fine cubes, nose first: `u` steps back along the body, `v` across it (centred on
// -0.5) and `w` up from the belly. Part 0 is body, 1 head, 2 tail (which flexes with the beat).
const FISH_U = [0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 6, 6, 6, 6];
const FISH_V = [
  -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -0.5, -1, 0, -1, 0, -1, 0, -2, -1, 0, 1,
];
const FISH_W = [0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 1, 0, 0, 1, 0, 0, 1];
const FISH_PART = [1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2, 2, 2];
const FISH_CUBES = FISH_U.length;
/** Middle of the body along `u`, which the salmon's position names. */
const FISH_MID_U = 3;
/** Most a salmon turns from straight upstream, in radians. */
const MAX_YAW = 0.6;

// The leaf as fine tiles floating flat, stem upstream: `u` downstream, `v` across; part 1 blade,
// 2 midrib and stem.
const LEAF_U = [0, 1, 1, 1, 2, 2, 2, 3];
const LEAF_V = [0, -1, 0, 1, -1, 0, 1, 0];
const LEAF_PART = [2, 1, 2, 1, 1, 2, 1, 1];

/** Kinds of depth-sorted drawable. */
const D_FALL = 0;
const D_BOULDER = 1;
const D_FISH = 2;
const D_LEAF = 3;
/** Falls (one per weir), boulders (at most one per pool), salmon and the leaf. */
const MAX_DRAWABLES = 2 * MAX_POOLS + MAX_FISH + 1;

/** The ladder's terrain, a cell kind per cube, painted onto a cached layer. */
class LadderTerrain {
  private cells = new Uint8Array(0);
  private sizeX = 0;
  private sizeY = 0;
  private sizeZ = 0;
  private readonly layer = new VoxelLayerCache();
  private layoutVersion = -1;
  private stale = true;
  private faces: readonly (IsoFaces | null)[] = [];
  private readonly paint = (c: Ctx2D) => this.paintCells(c);

  constructor(
    private readonly ladder: FishLadder,
    private readonly view: IsoView,
  ) {}

  /** Takes the fills for each cell kind (indexed by `T`), repainting with them on the next draw. */
  setFaces(faces: readonly (IsoFaces | null)[]): void {
    this.faces = faces;
    this.stale = true;
  }

  /**
   * Draws the terrain over the whole `width`×`height` canvas, rebuilding it if the ladder has
   * been laid out again and repainting it if that, the fills or (`viewChanged`) the view changed.
   */
  draw(ctx: CanvasRenderingContext2D, width: number, height: number, viewChanged: boolean): void {
    this.rebuild();
    this.layer.blit(ctx, width, height, this.stale || viewChanged, this.paint);
    this.stale = false;
  }

  /** Rebuilds the cells from the ladder's layout, if it changed. */
  private rebuild(): void {
    const ladder = this.ladder;
    if (ladder.layoutVersion === this.layoutVersion) return;
    this.layoutVersion = ladder.layoutVersion;
    this.stale = true;
    const sx = ladder.length;
    const sy = BANK_WIDTH + RIVER_WIDTH;
    const sz = ladder.poolSurface[0] + 2;
    this.sizeX = sx;
    this.sizeY = sy;
    this.sizeZ = sz;
    if (this.cells.length < sx * sy * sz) this.cells = new Uint8Array(sx * sy * sz);
    this.cells.fill(T.empty, 0, sx * sy * sz);
    for (let k = 0; k < ladder.poolCount; k++) {
      const surface = ladder.poolSurface[k];
      const weir = ladder.openWaterEnd(k);
      for (let x = ladder.poolStart[k]; x < ladder.poolEnd[k]; x++) {
        const lip = x >= weir;
        for (let y = -BANK_WIDTH; y < RIVER_WIDTH; y++) {
          if (y < 0) {
            // The far bank, rising toward the back.
            const top = surface + (y === -BANK_WIDTH ? 1 : 0);
            for (let z = 0; z < top; z++) this.set(x, y, z, T.stone);
            this.set(x, y, top, T.bankTop);
            continue;
          }
          const bed = surface - WATER_DEPTH;
          for (let z = 0; z < bed; z++) this.set(x, y, z, T.stone);
          const inNotch = y >= ladder.notchY[k] && y < ladder.notchY[k] + NOTCH_WIDTH;
          if (lip && !inNotch) {
            for (let z = bed; z < surface - 1; z++) this.set(x, y, z, T.stone);
            this.set(x, y, surface - 1, T.cap);
          } else {
            for (let z = bed; z < surface - 1; z++) this.set(x, y, z, T.deep);
            this.set(x, y, surface - 1, lip ? T.spill : T.water);
          }
        }
      }
    }
  }

  private set(x: number, y: number, z: number, kind: number): void {
    this.cells[(x * this.sizeY + (y + BANK_WIDTH)) * this.sizeZ + z] = kind;
  }

  private at(x: number, iy: number, z: number): number {
    if (x >= this.sizeX || iy >= this.sizeY || z >= this.sizeZ) return T.empty;
    return this.cells[(x * this.sizeY + iy) * this.sizeZ + z];
  }

  /** Every cube's visible faces, back to front: x, then y, then z, ascending. */
  private paintCells(ctx: Ctx2D): void {
    const { sizeX, sizeY, sizeZ } = this;
    for (let x = 0; x < sizeX; x++) {
      for (let iy = 0; iy < sizeY; iy++) {
        for (let z = 0; z < sizeZ; z++) {
          const kind = this.at(x, iy, z);
          if (kind === T.empty) continue;
          let mask = 0;
          if (this.at(x, iy, z + 1) === T.empty) mask |= FACE_TOP;
          if (this.at(x, iy + 1, z) === T.empty) mask |= FACE_LEFT;
          if (this.at(x + 1, iy, z) === T.empty) mask |= FACE_RIGHT;
          if (mask) this.view.cube(ctx, x, iy - BANK_WIDTH, z, 1, this.faces[kind]!, mask);
        }
      }
    }
  }
}

/**
 * Draws its `FishLadder` as isometric voxels in the theme's colours, scaled to fit the canvas.
 * `setPalette` must be called before the first `render`. Allocates nothing per frame.
 */
export class FishLadderRenderer {
  private readonly view = new IsoView();
  private readonly terrain: LadderTerrain;
  private palette!: SalmonRunPalette;
  private fitVersion = -1;
  private fitWidth = 0;
  private fitHeight = 0;

  // Depth-sorted drawables, and a salmon's cubes, in buffers sized once.
  private drawCount = 0;
  private readonly drawKind = new Int32Array(MAX_DRAWABLES);
  private readonly drawIndex = new Int32Array(MAX_DRAWABLES);
  private readonly drawDepth = new Float32Array(MAX_DRAWABLES);
  private readonly cubeX = new Float32Array(FISH_CUBES);
  private readonly cubeY = new Float32Array(FISH_CUBES);
  private readonly cubeZ = new Float32Array(FISH_CUBES);
  private readonly cubeOrder = new Int32Array(FISH_CUBES);

  constructor(private readonly ladder: FishLadder) {
    this.terrain = new LadderTerrain(ladder, this.view);
  }

  /** Takes the theme's fills; everything `render` needs from them is built here. */
  setPalette(p: SalmonRunPalette): void {
    this.palette = p;
    const faces: (IsoFaces | null)[] = new Array(7).fill(null);
    faces[T.stone] = p.stone;
    faces[T.cap] = p.cap;
    faces[T.bankTop] = p.bank;
    faces[T.deep] = p.deep;
    faces[T.water] = p.water;
    faces[T.spill] = p.spill;
    this.terrain.setFaces(faces);
  }

  render(ctx: CanvasRenderingContext2D, width: number, height: number): void {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    const ladder = this.ladder;
    const view = this.view;
    const p = this.palette;
    let viewChanged = false;
    if (
      ladder.layoutVersion !== this.fitVersion ||
      width !== this.fitWidth ||
      height !== this.fitHeight
    ) {
      viewChanged = view.fit(
        width,
        height,
        0,
        ladder.length,
        -BANK_WIDTH,
        RIVER_WIDTH,
        0,
        ladder.poolSurface[0] + 2,
      );
      this.fitVersion = ladder.layoutVersion;
      this.fitWidth = width;
      this.fitHeight = height;
    }
    ctx.clearRect(0, 0, width, height);
    this.terrain.draw(ctx, width, height, viewChanged);
    // Under reduced motion a beat lifts the light on the water instead of making waves: a wash
    // over each pool, so the cached terrain never needs repainting for it.
    const light = Math.min(LIGHT_STEPS, Math.round(ladder.glow * LIGHT_STEPS));
    if (light > 0) {
      for (let k = 0; k < ladder.poolCount; k++) {
        const z = ladder.poolSurface[k];
        const end = ladder.openWaterEnd(k);
        view.plane(ctx, ladder.poolStart[k], 0, end, RIVER_WIDTH, z, p.lightWash[light - 1]);
      }
    }

    this.drawGlints(ctx);
    this.drawRipples(ctx);
    this.sortDrawables();
    for (let d = 0; d < this.drawCount; d++) {
      const i = this.drawIndex[d];
      switch (this.drawKind[d]) {
        case D_FALL:
          this.drawFall(ctx, i);
          break;
        case D_BOULDER: {
          const z = ladder.poolSurface[ladder.boulderPool[i]];
          view.cube(ctx, ladder.boulderX[i], ladder.boulderY[i], z, 1, p.cap);
          break;
        }
        case D_FISH:
          this.drawSalmon(ctx, i);
          break;
        default:
          this.drawLeaf(ctx);
      }
    }
  }

  /** `FOAM_STEPS` rung for brightness `b` in [0, 1], or -1 when too faint to show. */
  private rung(b: number): number {
    return Math.min(FOAM_STEPS - 1, Math.round(b * FOAM_STEPS) - 1);
  }

  private addDrawable(kind: number, index: number, depth: number): void {
    // Insertion into depth order, back (smallest x + y + z) first.
    let d = this.drawCount++;
    while (d > 0 && this.drawDepth[d - 1] > depth) {
      this.drawKind[d] = this.drawKind[d - 1];
      this.drawIndex[d] = this.drawIndex[d - 1];
      this.drawDepth[d] = this.drawDepth[d - 1];
      d--;
    }
    this.drawKind[d] = kind;
    this.drawIndex[d] = index;
    this.drawDepth[d] = depth;
  }

  private sortDrawables(): void {
    const ladder = this.ladder;
    this.drawCount = 0;
    for (let k = 0; k < ladder.poolCount - 1; k++) {
      const depth = ladder.poolEnd[k] + 0.5 + ladder.notchCenter(k) + ladder.poolSurface[k + 1];
      this.addDrawable(D_FALL, k, depth);
    }
    for (let b = 0; b < ladder.boulderCount; b++) {
      const depth =
        ladder.boulderX[b] + ladder.boulderY[b] + 1 + ladder.poolSurface[ladder.boulderPool[b]];
      this.addDrawable(D_BOULDER, b, depth);
    }
    for (let i = 0; i < ladder.fishCount; i++) {
      this.addDrawable(D_FISH, i, ladder.fishX[i] + ladder.fishY[i] + ladder.fishZ[i]);
    }
    if (ladder.leafActive) {
      this.addDrawable(D_LEAF, 0, ladder.leafX + ladder.leafY + ladder.leafZ);
    }
  }

  /** Current glints: short runs of fine tiles drifting on each pool. */
  private drawGlints(ctx: Ctx2D): void {
    const ladder = this.ladder;
    const a = ladder.audio;
    const k = this.rung((0.35 + 0.35 * a.treble) * (0.4 + 0.6 * a.activity));
    if (k < 0) return;
    for (let g = 0; g < ladder.glintCount; g++) {
      const x = Math.floor(ladder.glintX[g] / FINE) * FINE;
      const y = Math.floor(ladder.glintY[g] / FINE) * FINE;
      const z = ladder.poolSurface[ladder.glintPool[g]];
      for (let s = 0; s < 3; s++)
        this.view.tile(ctx, x - s * FINE, y, z, FINE, this.palette.glint[k]);
    }
  }

  /** Ripples as rings of fine tiles on the pool they spread on, kept to its open water. */
  private drawRipples(ctx: Ctx2D): void {
    const ladder = this.ladder;
    for (let i = 0; i < MAX_RIPPLES; i++) {
      const k = this.rung(ladder.rippleAlpha(i));
      if (k < 0) continue;
      const cx = ladder.rippleX[i];
      const cy = ladder.rippleY[i];
      const rad = ladder.rippleRadius(i);
      const pool = ladder.poolAt(cx);
      const x0 = Math.max(ladder.poolStart[pool], Math.floor((cx - rad - FINE) / FINE) * FINE);
      const x1 = Math.min(ladder.openWaterEnd(pool), cx + rad + FINE);
      const y0 = Math.max(0, Math.floor((cy - rad - FINE) / FINE) * FINE);
      const y1 = Math.min(RIVER_WIDTH, cy + rad + FINE);
      const z = ladder.rippleZ[i];
      const fill = this.palette.ripple[k];
      for (let x = x0; x < x1; x += FINE) {
        for (let y = y0; y < y1; y += FINE) {
          const d = Math.hypot(x + FINE / 2 - cx, y + FINE / 2 - cy);
          if (Math.abs(d - rad) <= FINE * 0.6) this.view.tile(ctx, x, y, z, FINE, fill);
        }
      }
    }
  }

  /**
   * The water spilling through weir k's notch: a column of foam cubes down the step face, with
   * bands of brighter water running down it, and foam churning at its foot. It brightens with the
   * bass and with the reduced-motion light shift.
   */
  private drawFall(ctx: Ctx2D, k: number): void {
    const ladder = this.ladder;
    const p = this.palette;
    const a = ladder.audio;
    const base = (0.35 + 0.35 * a.bass) * (0.6 + 0.4 * a.activity) + 0.3 * ladder.glow;
    const x = ladder.poolEnd[k];
    const low = ladder.poolSurface[k + 1];
    const high = ladder.poolSurface[k];
    const phase = ladder.fallPhase;
    const n0 = ladder.notchY[k];
    for (let y = n0; y < n0 + NOTCH_WIDTH; y++) {
      for (let z = low; z < high; z++) {
        const band = (((high - z + phase + y * 0.7) % 3) + 3) % 3 < 1 ? 0.25 : 0;
        const r = Math.max(0, this.rung(Math.min(1, base + band)));
        this.view.cube(ctx, x, y, z, 1, p.foam[r], FACE_ALL);
      }
    }
    // Foam at the foot, flickering as the water lands.
    for (let s = 0; s < 4; s++) {
      const fy = n0 - FINE + s * FINE * 2;
      const flicker = (((phase * 1.7 + s * 1.3) % 2) + 2) % 2 < 1 ? 0.15 : -0.05;
      const r = this.rung(Math.min(1, base + flicker));
      if (r >= 0) this.view.tile(ctx, x + 1, fy, low, FINE, p.ripple[r]);
    }
  }

  /**
   * Salmon `i` as fine cubes, built from its model turned to its heading (always roughly
   * upstream), tilted to its leap's slope, with the tail flexing with its tail beat. Cubes off
   * either end of the ladder aren't drawn, so salmon appear from the front edge and leave over
   * the back.
   */
  private drawSalmon(ctx: Ctx2D, i: number): void {
    const ladder = this.ladder;
    const h = FISH_CUBE * ladder.fishScale[i];
    const yaw = Math.max(
      -MAX_YAW,
      Math.min(MAX_YAW, Math.atan2(ladder.fishVY[i], Math.max(-ladder.fishVX[i], 0.3))),
    );
    // Heading upstream (-x), turned by `yaw`; `side` is across the body.
    const hx = -Math.cos(yaw);
    const hy = Math.sin(yaw);
    const sx = -hy;
    const sy = hx;
    const pitch = ladder.fishPitch[i];
    const flex = Math.sin(ladder.fishTail[i]) * h;
    const fx = ladder.fishX[i];
    const fy = ladder.fishY[i];
    const fz = ladder.fishZ[i];
    let count = 0;
    for (let c = 0; c < FISH_CUBES; c++) {
      const along = (FISH_MID_U - FISH_U[c]) * h;
      let across = (FISH_V[c] + 0.5) * h;
      if (FISH_PART[c] === 2) across += flex * (FISH_U[c] - 4) * 0.6;
      const x = fx + hx * along + sx * across - h / 2;
      const y = fy + hy * along + sy * across - h / 2;
      const z = fz + FISH_W[c] * h + along * pitch;
      if (x < 0 || x + h > ladder.length) continue;
      this.cubeX[c] = x;
      this.cubeY[c] = y;
      this.cubeZ[c] = z;
      // Insertion into depth order.
      let o = count++;
      const depth = x + y + z;
      while (o > 0) {
        const prev = this.cubeOrder[o - 1];
        if (this.cubeX[prev] + this.cubeY[prev] + this.cubeZ[prev] <= depth) break;
        this.cubeOrder[o] = prev;
        o--;
      }
      this.cubeOrder[o] = c;
    }
    const p = this.palette;
    for (let o = 0; o < count; o++) {
      const c = this.cubeOrder[o];
      const faces = FISH_PART[c] === 1 ? p.fishHeadFaces : p.fishBodyFaces;
      this.view.cube(ctx, this.cubeX[c], this.cubeY[c], this.cubeZ[c], h, faces);
    }
  }

  /** The leaf: fine tiles floating flat, its midrib in the vein colour. */
  private drawLeaf(ctx: Ctx2D): void {
    const ladder = this.ladder;
    const x0 = ladder.leafX - 2 * FINE;
    const y0 = ladder.leafY - FINE / 2;
    for (let t = 0; t < LEAF_U.length; t++) {
      const x = x0 + LEAF_U[t] * FINE;
      if (x < 0 || x + FINE > ladder.length) continue;
      const fill = LEAF_PART[t] === 2 ? this.palette.leafVein : this.palette.leaf;
      this.view.tile(ctx, x, y0 + LEAF_V[t] * FINE, ladder.leafZ + 0.02, FINE, fill);
    }
  }
}
