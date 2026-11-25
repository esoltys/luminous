import {
  BANK_WIDTH,
  BankPlant,
  FISH_SIZE,
  type FishLadder,
  MAX_FISH,
  MAX_NOTCHES,
  MAX_POOLS,
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

/** Terrain cell kinds, indices into `terrainFaces`; 0 is empty. */
const T = {
  empty: 0,
  stone: 1,
  cap: 2,
  bankTop: 3,
  deep: 4,
  water: 5,
  spill: 6,
  trunk: 7,
  pine: 8,
  shrub: 9,
  autumnBush: 10,
} as const;

// A salmon as fine cubes, nose first: `u` steps back along the body, `v` across it (centred on
// -0.5) and `w` up from the belly. Part 0 is body, 1 head, 2 tail (a single vertical fin that
// flexes with the beat).
const FISH_U = [0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 6, 6];
const FISH_V = [
  -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -0.5, -1, 0, -1, 0, -0.5, -0.5, -0.5,
];
const FISH_W = [0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 2, 0, 0, 1, 1, 0, 0, 1];
const FISH_PART = [1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2];
const FISH_CUBES = FISH_U.length;
/** Middle of the body along `u`, which the salmon's position names. */
const FISH_MID_U = 3;
/** Most a salmon turns from straight upstream, in radians. */
const MAX_YAW = 0.6;

/** Edge of the leaf's tiles, matching the frog's resolution. */
const LEAF_TILE = 0.22;
// Canadian flag maple leaf floating flat, right edge downstream: `u` base-to-apex, `v` across;
// part 1 blade, 2 stem.
const LEAF_U = [
  // Stem
  -3, -2, -1,
  // Basal points on either side of the stem
  -0.5, -0.5,
  // Base
  0, 0, 0, 0, 0, 0, 0,
  // Waist above basal points
  1, 1, 1, 1, 1,
  // Widest side wings
  2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
  // Upper wings
  3, 3, 3, 3, 3, 3, 3, 3, 3,
  // Side lobes + center base (notched at v = ±1)
  4, 4, 4, 4, 4,
  // Upper side tips + center crown
  5, 5, 5, 5, 5,
  // Center crown + apex
  6, 6, 6, 7,
];
const LEAF_V = [
  0, 0, 0, -3, 3, -3, -2, -1, 0, 1, 2, 3, -2, -1, 0, 1, 2, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, -4,
  -3, -2, -1, 0, 1, 2, 3, 4, -3, -2, 0, 2, 3, -3, -1, 0, 1, 3, -1, 0, 1, 0,
];
const LEAF_PART = [
  2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

/** Kinds of depth-sorted drawable. */
const D_FALL = 0;
const D_BOULDER = 1;
const D_FISH = 2;
const D_LEAF = 3;
const D_BEAR = 4;
const D_TOP_BEAR = 5;
const D_FROG = 6;
/** Falls (up to MAX_NOTCHES per weir), boulders (at most one per pool), salmon, leaf, frog, and bears. */
const MAX_DRAWABLES = MAX_POOLS * (MAX_NOTCHES + 1) + MAX_FISH + 4;

/** Edge of a voxel bear head's cubes, in terrain units. */
const BEAR_CUBE = 0.52;
// Bear head facing downstream (+u): part 0 fur, 1 snout, 2 dark (eyes/nose), 3 lower jaw.
// 3x2x2 head block + 2 ears + 2 eyes + upper snout + nose + 2 lower-jaw cubes.
const BEAR_U = [-1, -1, -1, 0, 0, 0, -1, -1, -1, 0, 0, 0, -1, -1, 0.25, 0.25, 1, 1, 1.6, 0.5, 1.2];
const BEAR_V = [-1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 1, -0.8, 0.8, -0.4, 0.4, 0, 0, 0];
const BEAR_W = [1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 3, 3, 2, 2, 1.4, 1.4, 1.7, 0.2, 0.2];
const BEAR_PART = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 1, 1, 2, 3, 3];
const BEAR_CUBES = BEAR_U.length;

/** Edge of the voxel frog's cubes perched on a boulder. */
const FROG_CUBE = 0.24;
// Sitting frog facing downstream (+u) with separated bulging eyes and splayed hind legs:
// part 0 green body/legs, 1 eyes.
const FROG_U = [
  // Torso (3x2x1) + snout
  -1, -1, -1, 0, 0, 0, 1, 1,
  // Raised back + head bridge
  -0.5, 0.2,
  // Separated eye bumps (green base) + pupils on top
  0.4, 0.4, 0.4, 0.4,
  // Splayed hind legs (thighs + feet)
  -1.2, -1.2, -0.4, -0.4,
];
const FROG_V = [-1, 0, 1, -1, 0, 1, -0.5, 0.5, 0, 0, -1.1, 1.1, -1.1, 1.1, -1.8, 1.8, -1.8, 1.8];
const FROG_W = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1.8, 1.8, 0, 0, 0.5, 0.5];
const FROG_PART = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0];
const FROG_CUBES = FROG_U.length;

/** Edge of the sitting top-pool bear's cubes, in terrain units. */
const TOP_BEAR_CUBE = 0.62;
/** Vertical headroom above the top pool surface for trees and leaping salmon. */
const SCENE_HEADROOM = 6;
// Sitting chubby bear facing downstream (+u):
// part 0 body/paws, 1 snout, 2 dark (eyes/nose), 3 head/ears, 4 shoulders.
const TOP_BEAR_U = [
  // Wide haunches & lower belly in water (3x4 base, 2 layers)
  -1, -1, -1, -1, 0, 0, 0, 0, 1, 1, 1, 1, -1, -1, -1, -1, 0, 0, 0, 0, 1, 1, 1, 1,
  // Front paws resting forward
  1.8, 1.8,
  // Broad upper shoulders
  -0.4, -0.4, -0.4, -0.4, 0.5, 0.5, 0.5, 0.5,
  // Head
  0, 0, 0, 0.8, 0.8, 0.8,
  // Ears, eyes, snout, nose
  -0.2, -0.2, 1.0, 1.0, 1.6, 1.6, 2.2,
];
const TOP_BEAR_V = [
  -1.5, -0.5, 0.5, 1.5, -1.5, -0.5, 0.5, 1.5, -1.5, -0.5, 0.5, 1.5, -1.5, -0.5, 0.5, 1.5, -1.5,
  -0.5, 0.5, 1.5, -1.5, -0.5, 0.5, 1.5, -1.2, 1.2, -1.5, -0.5, 0.5, 1.5, -1.5, -0.5, 0.5, 1.5, -0.8,
  0, 0.8, -0.8, 0, 0.8, -0.9, 0.9, -0.7, 0.7, -0.35, 0.35, 0,
];
const TOP_BEAR_W = [
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 2, 2, 2, 2, 2, 2, 2,
  2, 3, 3, 3, 3, 3, 3, 4, 4, 3.6, 3.6, 2.9, 2.9, 3.2,
];
const TOP_BEAR_PART = [
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 4, 4, 4, 4, 4,
  4, 3, 3, 3, 3, 3, 3, 3, 3, 2, 2, 1, 1, 2,
];
const TOP_BEAR_CUBES = TOP_BEAR_U.length;

const MAX_MODEL_CUBES = Math.max(FISH_CUBES, BEAR_CUBES, FROG_CUBES, TOP_BEAR_CUBES);

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
    const sz = ladder.poolSurface[0] + SCENE_HEADROOM;
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
            // The far bank, rising toward the back, with sparse trees and shrubs on top.
            const top = surface + (y === -BANK_WIDTH ? 1 : 0);
            for (let z = 0; z < top; z++) this.set(x, y, z, T.stone);
            this.set(x, y, top, T.bankTop);
            const plant = ladder.bankPlant[x * BANK_WIDTH + (y + BANK_WIDTH)];
            if (plant === BankPlant.SHRUB) {
              this.set(x, y, top + 1, T.shrub);
            } else if (plant === BankPlant.AUTUMN_BUSH) {
              this.set(x, y, top + 1, T.autumnBush);
            } else if (plant === BankPlant.PINE) {
              this.set(x, y, top + 1, T.trunk);
              this.set(x, y, top + 2, T.pine);
              this.set(x, y, top + 3, T.pine);
              if (x % 2 === 0) this.set(x, y, top + 4, T.pine);
            }
            continue;
          }
          const bed = surface - WATER_DEPTH;
          for (let z = 0; z < bed; z++) this.set(x, y, z, T.stone);
          const inNotch = ladder.isInNotch(k, y);
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
  private readonly cubeX = new Float32Array(MAX_MODEL_CUBES);
  private readonly cubeY = new Float32Array(MAX_MODEL_CUBES);
  private readonly cubeZ = new Float32Array(MAX_MODEL_CUBES);
  private readonly cubeOrder = new Int32Array(MAX_MODEL_CUBES);
  private readonly cubeFaces: (IsoFaces | null)[] = new Array(MAX_MODEL_CUBES).fill(null);

  constructor(private readonly ladder: FishLadder) {
    this.terrain = new LadderTerrain(ladder, this.view);
  }

  /** Takes the theme's fills; everything `render` needs from them is built here. */
  setPalette(p: SalmonRunPalette): void {
    this.palette = p;
    const faces: (IsoFaces | null)[] = new Array(11).fill(null);
    faces[T.stone] = p.stone;
    faces[T.cap] = p.cap;
    faces[T.bankTop] = p.bank;
    faces[T.deep] = p.deep;
    faces[T.water] = p.water;
    faces[T.spill] = p.spill;
    faces[T.trunk] = p.trunk;
    faces[T.pine] = p.pine;
    faces[T.shrub] = p.shrub;
    faces[T.autumnBush] = p.autumnBush;
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
        ladder.poolSurface[0] + SCENE_HEADROOM,
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
        case D_FROG:
          this.drawFrog(ctx, ladder.frogX, ladder.frogY, ladder.frogZ);
          break;
        case D_FISH:
          this.drawSalmon(ctx, i);
          break;
        case D_BEAR:
          this.drawBear(ctx);
          break;
        case D_TOP_BEAR:
          this.drawTopBear(ctx);
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
      const count = ladder.notchCount[k];
      for (let n = 0; n < count; n++) {
        const depth =
          ladder.poolEnd[k] + 0.5 + ladder.notchCenter(k, n) + ladder.poolSurface[k + 1];
        this.addDrawable(D_FALL, k * MAX_NOTCHES + n, depth);
      }
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
    if (ladder.bearActive && ladder.bearRise > 0) {
      this.addDrawable(D_BEAR, 0, ladder.bearX + ladder.bearY + ladder.bearZ + 1);
    }
    this.addDrawable(D_TOP_BEAR, 0, ladder.topBearX + ladder.topBearY + ladder.poolSurface[0]);
    if (ladder.frogVisible) {
      this.addDrawable(D_FROG, 0, ladder.frogX + ladder.frogY + ladder.frogZ);
    }
  }

  /** True when canvas CSS point `(px, py)` hits the generous hitbox around the frog and its rock. */
  hitTestFrog(px: number, py: number): boolean {
    const ladder = this.ladder;
    if (!ladder.frogVisible) return false;
    const fx = this.view.screenX(ladder.frogX, ladder.frogY);
    const fy = this.view.screenY(ladder.frogX, ladder.frogY, ladder.frogZ - 0.15);
    const r = Math.max(32, this.view.unit * 2.2);
    return Math.abs(px - fx) <= r && Math.abs(py - fy) <= r;
  }

  /** Current glints: short runs of fine tiles drifting on each pool. */
  private drawGlints(ctx: Ctx2D): void {
    const ladder = this.ladder;
    const a = ladder.audio;
    const k = this.rung((0.35 + 0.35 * a.treble) * (0.4 + 0.6 * a.activity));
    if (k < 0) return;
    for (let g = 0; g < ladder.glintCount; g++) {
      const pool = ladder.glintPool[g];
      const start = ladder.poolStart[pool];
      const end = ladder.openWaterEnd(pool);
      const x = Math.floor(ladder.glintX[g] / FINE) * FINE;
      const y = Math.floor(ladder.glintY[g] / FINE) * FINE;
      const z = ladder.poolSurface[pool];
      for (let s = 0; s < 3; s++) {
        const gx = x - s * FINE;
        if (gx >= start && gx + FINE <= end) {
          this.view.tile(ctx, gx, y, z, FINE, this.palette.glint[k]);
        }
      }
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
  private drawFall(ctx: Ctx2D, fallIdx: number): void {
    const ladder = this.ladder;
    const k = Math.floor(fallIdx / MAX_NOTCHES);
    const n = fallIdx % MAX_NOTCHES;
    const slot = k * MAX_NOTCHES + n;
    const p = this.palette;
    const a = ladder.audio;
    const base = (0.35 + 0.35 * a.bass) * (0.6 + 0.4 * a.activity) + 0.3 * ladder.glow;
    const x = ladder.poolEnd[k];
    const low = ladder.poolSurface[k + 1];
    const high = ladder.poolSurface[k];
    const phase = ladder.fallPhase;
    const n0 = ladder.notchY[slot];
    const nw = ladder.notchW[slot];
    for (let y = n0; y < n0 + nw; y++) {
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

  /** Inserts a voxel cube into the shared depth-sorted buffer; returns the updated count. */
  private pushCube(count: number, x: number, y: number, z: number, faces: IsoFaces): number {
    this.cubeX[count] = x;
    this.cubeY[count] = y;
    this.cubeZ[count] = z;
    this.cubeFaces[count] = faces;
    let o = count;
    const depth = x + y + z;
    while (o > 0) {
      const prev = this.cubeOrder[o - 1];
      if (this.cubeX[prev] + this.cubeY[prev] + this.cubeZ[prev] <= depth) break;
      this.cubeOrder[o] = prev;
      o--;
    }
    this.cubeOrder[o] = count;
    return count + 1;
  }

  /** Renders all queued voxel cubes in back-to-front order. */
  private flushCubes(ctx: Ctx2D, count: number, h: number): void {
    for (let o = 0; o < count; o++) {
      const c = this.cubeOrder[o];
      this.view.cube(ctx, this.cubeX[c], this.cubeY[c], this.cubeZ[c], h, this.cubeFaces[c]!);
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
    const h = FISH_SIZE * ladder.fishScale[i];
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
    const p = this.palette;
    let count = 0;
    for (let c = 0; c < FISH_CUBES; c++) {
      const along = (FISH_MID_U - FISH_U[c]) * h;
      let across = (FISH_V[c] + 0.5) * h;
      if (FISH_PART[c] === 2) across += flex * (FISH_U[c] - 4) * 0.6;
      const x = fx + hx * along + sx * across - h / 2;
      const y = fy + hy * along + sy * across - h / 2;
      const z = fz + FISH_W[c] * h + along * pitch;
      if (x < 0 || x + h > ladder.length) continue;
      count = this.pushCube(count, x, y, z, FISH_PART[c] === 1 ? p.fishHeadFaces : p.fishBodyFaces);
    }
    this.flushCubes(ctx, count, h);
  }

  /** The leaf: Canadian maple leaf rotated so its right edge leads downstream. */
  private drawLeaf(ctx: Ctx2D): void {
    const ladder = this.ladder;
    const tSize = LEAF_TILE;
    const x0 = ladder.leafX - tSize / 2;
    const y0 = ladder.leafY + 2 * tSize;
    for (let t = 0; t < LEAF_U.length; t++) {
      const x = x0 + LEAF_V[t] * tSize;
      if (x < 0 || x + tSize > ladder.length) continue;
      const fill = LEAF_PART[t] === 2 ? this.palette.leafVein : this.palette.leaf;
      this.view.tile(ctx, x, y0 - LEAF_U[t] * tSize, ladder.leafZ + 0.02, tSize, fill);
    }
  }

  /** Rare voxel bear head rising at the weir, jaws open until it snaps a leaping salmon. */
  private drawBear(ctx: Ctx2D): void {
    const ladder = this.ladder;
    const h = BEAR_CUBE;
    const bx = ladder.bearX;
    const by = ladder.bearY;
    const baseZ = ladder.bearZ - (1 - ladder.bearRise) * 2.2;
    const jawLift = ladder.bearJaw > 0.5 ? 0 : 0.75;
    const p = this.palette;
    let count = 0;
    for (let c = 0; c < BEAR_CUBES; c++) {
      const x = bx + BEAR_U[c] * h - h / 2;
      const y = by + BEAR_V[c] * h - h / 2;
      const part = BEAR_PART[c];
      const w = part === 3 ? BEAR_W[c] + jawLift : BEAR_W[c];
      const z = baseZ + w * h;
      if (z < ladder.bearZ - 0.5) continue;
      const faces = part === 2 ? p.bearDark : part === 1 || part === 3 ? p.bearSnout : p.bearFur;
      count = this.pushCube(count, x, y, z, faces);
    }
    this.flushCubes(ctx, count, h);
  }

  /** Tiny voxel frog perched on its boulder. */
  private drawFrog(ctx: Ctx2D, fx: number, fy: number, fz: number): void {
    const h = FROG_CUBE;
    const p = this.palette;
    let count = 0;
    for (let c = 0; c < FROG_CUBES; c++) {
      const x = fx + FROG_U[c] * h - h / 2;
      const y = fy + FROG_V[c] * h - h / 2;
      const z = fz + FROG_W[c] * h;
      count = this.pushCube(count, x, y, z, FROG_PART[c] === 1 ? p.frogEye : p.frogBody);
    }
    this.flushCubes(ctx, count, h);
  }

  /** Bear sitting in the top pool: gentle breathing/scanning idle, plus a chomp nod on catch. */
  private drawTopBear(ctx: Ctx2D): void {
    const ladder = this.ladder;
    const h = TOP_BEAR_CUBE;
    const bx = ladder.topBearX;
    const by = ladder.topBearY;
    const bz = ladder.poolSurface[0] - 0.15;
    const nod = ladder.topBearNod;
    const breath = ladder.topBearBreath;
    const lookY = ladder.topBearLookY;
    const p = this.palette;
    let count = 0;
    for (let c = 0; c < TOP_BEAR_CUBES; c++) {
      const part = TOP_BEAR_PART[c];
      const isHead = part === 1 || part === 2 || part === 3;
      const drop = part === 0 ? 0 : nod * (isHead ? 1.4 : 0.75) - breath * (isHead ? 1.2 : 0.7);
      const lean = part === 0 ? 0 : nod * (isHead ? 0.7 : 0.35);
      const sway = isHead ? lookY : 0;
      const x = bx + TOP_BEAR_U[c] * h - h / 2 + lean;
      const y = by + TOP_BEAR_V[c] * h - h / 2 + sway;
      const z = bz + TOP_BEAR_W[c] * h - drop;
      const faces = part === 2 ? p.bearDark : part === 1 ? p.bearSnout : p.bearFur;
      count = this.pushCube(count, x, y, z, faces);
    }
    this.flushCubes(ctx, count, h);
  }
}
