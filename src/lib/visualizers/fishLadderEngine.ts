import { RiverAudio, SeededRandom, seedFromKey } from "./salmonRunEngine";
import type { VisualizerFrame } from "./types";

// The fish ladder is laid out in scene units, one per terrain cube: x runs downstream along the
// river (from the top pool at the back, x = 0, to the front edge at `length`), y across it (0 at
// the far bank to `RIVER_WIDTH` at the near side) and z up. The ladder steps down toward the
// viewer, so a pool sits `DROP` above the one in front of it. Nothing here depends on the canvas:
// the renderer scales the whole scene to fit.

/** Water cells across the river. */
export const RIVER_WIDTH = 20;
/** Cells of bank along the far side, at y in [-BANK_WIDTH, 0). */
export const BANK_WIDTH = 2;
/** Height of each step, from one pool's surface to the next pool's. */
export const DROP = 4;
/** Cubes of water in a pool, and of stone under the bottom pool's water. */
export const WATER_DEPTH = 2;
const BED_DEPTH = 1;
/** Default width of a single notch in a weir that the water spills through. */
const NOTCH_WIDTH = 3;
/** Maximum number of overflow notches per weir step. */
export const MAX_NOTCHES = 3;
/** Pools, boulders, fish and current glints the ladder holds. */
export const MAX_POOLS = 4;
const MAX_BOULDERS = MAX_POOLS;
const MAX_LENGTH = MAX_POOLS * 9 + 8;
export const MAX_FISH = 8;
const GLINTS_PER_POOL = 3;
const MAX_GLINTS = MAX_POOLS * GLINTS_PER_POOL;
/** Ripples spreading at once; a new one replaces the oldest beyond this. */
export const MAX_RIPPLES = 12;
const RIPPLE_LIFE_S = 1.4;
const RIPPLE_GROWTH = 1.7;

/** Seconds a leap lasts, and how high its arc bows above the straight line from take-off. */
export const LEAP_S = 0.9;
const LEAP_BOW = 4.0;
/** A short leap: it bows below the lip and drops back into the pool it left. */
const SHORT_LEAP_S = 0.7;
const SHORT_LEAP_BOW = 1.8;
/** Transient strength a leap needs to clear the weir; weaker ones fall short. */
const CLEAR_STRENGTH = 0.7;
/** Seconds a salmon waits below a weir before it leaps on its own, drawn per wait. */
export const PATIENCE_S: readonly [number, number] = [6, 12];

/** Speed of the surface current at full activity, in cells per second. */
const FLOW_SPEED = 1.5;
/** Base cube size of a salmon before its individual scale. */
export const FISH_SIZE = 0.38;
/** Swimming speed: a slow base plus a share driven by mid/treble energy, in cells per second. */
const SWIM_BASE = 0.45;
const SWIM_ENERGY = 0.8;
/** Tail beats per second: a base plus a share driven by mid/treble energy. */
const TAIL_BASE_HZ = 1.4;
const TAIL_ENERGY_HZ = 2.6;
/** Under reduced motion salmon swim, and the leaf drifts, this much slower. */
const REDUCED_PACE = 0.6;
const LEAF_REDUCED_PACE = 0.55;
/** Seconds the leaf takes to float the whole ladder. */
const LEAF_CROSS_S = 22;
/** Where a salmon waits for its leap: this far into its pool from the step it will climb. */
const WAIT_DEPTH = 2.5;
/** How close to its waiting spot a salmon must be to leap. */
const READY_REACH = 1.2;
/** Clearance a salmon keeps from the step above it, from its weir, and from the banks. */
const STEP_CLEARANCE = 2.3;
const WEIR_CLEARANCE = 3;
const BANK_CLEARANCE = 0.7;
/** Reduced-motion "light shift" a beat leaves on the water, and its decay per second. */
const GLOW_PER_ONSET = 0.6;
const GLOW_DECAY = 1.2;
/** Rare voxel bear pop-up: probability per clearing leap, cooldown, and sink timings. */
const BEAR_CHANCE = 0.12;
const BEAR_COOLDOWN_S = 14;
const BEAR_HOLD_S = 0.45;
const BEAR_SINK_S = 0.35;
/** Duration of one smooth down-and-up bear chomp nod when eating a salmon. */
const TOP_BEAR_NOD_S = 0.32;
/** Duration of the frog's jump arc and how long it rests in the water before hopping back. */
const FROG_JUMP_S = 0.55;
const FROG_JUMP_BOW = 1.6;
const FROG_WATER_S = 2.5;
/** Vegetation kinds placed along the far bank. */
export const BankPlant = {
  NONE: 0,
  SHRUB: 1,
  PINE: 2,
  AUTUMN_BUSH: 3,
} as const;

/**
 * A salmon run up a fish ladder: a per-track flight of pools, each a step above the next, with
 * weirs whose notches spill water down the steps; a school of salmon working their way up; and
 * the ripples, current glints and drifting leaf on the water. The audio drives it: the school's
 * pace follows mid and treble, a treble transient sends the salmon waiting at a weir up a step,
 * and beats drop ripples (or, under reduced motion, lift the light on the water).
 *
 * A salmon only ever changes pool by leaping up one step (`fishPool` drops by one as it lands);
 * a short leap lands back in the pool it left. Salmon that swim out of the top pool rejoin the
 * run below the bottom one. Every array is allocated once; `push` and `step` only write into them,
 * and the renderer reads the public fields directly.
 */
export class FishLadder {
  readonly audio = new RiverAudio();

  /** Pools from the top (0, furthest back) down to the front; pool k spans x in [start, end). */
  poolCount = 0;
  readonly poolStart = new Float32Array(MAX_POOLS);
  readonly poolEnd = new Float32Array(MAX_POOLS);
  /** Height of each pool's water surface (the top of its water cubes). */
  readonly poolSurface = new Float32Array(MAX_POOLS);
  /**
   * Overflow notches in pool k's weir (its last row, x = end - 1): `notchCount[k]` in [1, MAX_NOTCHES],
   * each at `notchY[k * MAX_NOTCHES + n]` with width `notchW[k * MAX_NOTCHES + n]`.
   */
  readonly notchCount = new Int32Array(MAX_POOLS);
  readonly notchY = new Int32Array(MAX_POOLS * MAX_NOTCHES);
  readonly notchW = new Int32Array(MAX_POOLS * MAX_NOTCHES);
  /** Downstream end of the whole ladder. */
  length = 0;

  /** Boulders, one cell each, standing a cube above their pool's surface. */
  boulderCount = 0;
  readonly boulderX = new Int32Array(MAX_BOULDERS);
  readonly boulderY = new Int32Array(MAX_BOULDERS);
  readonly boulderPool = new Int32Array(MAX_BOULDERS);
  /** Index of the boulder a voxel frog sits on, or -1 if none. */
  frogBoulder = -1;
  /** Frog position in scene units (on its boulder, arcing, or floating in the pool). */
  frogX = 0;
  frogY = 0;
  frogZ = 0;
  private frogInWater = false;
  private frogJumpT = 0;
  private frogWaterT = 0;
  private frogFromX = 0;
  private frogFromY = 0;
  private frogFromZ = 0;
  private frogToX = 0;
  private frogToY = 0;
  private frogToZ = 0;

  /** True when the frog is visible (on its boulder or mid-jump, hidden while in the water). */
  get frogVisible(): boolean {
    return this.frogBoulder >= 0 && (!this.frogInWater || this.frogJumpT > 0);
  }
  /** Sitting bear in the top pool that nods when eating a salmon and breathes/looks idly. */
  topBearX = 0;
  topBearY = 0;
  topBearNod = 0;
  topBearBreath = 0;
  topBearLookY = 0;
  topBearIdle = 0;
  private topBearNodT = 0;
  /**
   * Vegetation along the far bank (`BankPlant` values), indexed by `x * BANK_WIDTH + (y + BANK_WIDTH)`.
   */
  readonly bankPlant = new Uint8Array(MAX_LENGTH * BANK_WIDTH);

  fishCount = 0;
  readonly fishPool = new Int32Array(MAX_FISH);
  /** Position of the salmon's middle; z is the height of its belly. */
  readonly fishX = new Float32Array(MAX_FISH);
  readonly fishY = new Float32Array(MAX_FISH);
  readonly fishZ = new Float32Array(MAX_FISH);
  /** Swimming velocity in cells per second; negative x is upstream. */
  readonly fishVX = new Float32Array(MAX_FISH);
  readonly fishVY = new Float32Array(MAX_FISH);
  /** Nose-up slope of the body while leaping, in [-1.2, 1.2]; 0 while swimming. */
  readonly fishPitch = new Float32Array(MAX_FISH);
  /** Tail-beat phase in radians. */
  readonly fishTail = new Float32Array(MAX_FISH);
  /** Size multiplier, so the school isn't uniform. */
  readonly fishScale = new Float32Array(MAX_FISH);
  /** Length of the current leap in seconds; 0 while swimming. */
  readonly fishLeapS = new Float32Array(MAX_FISH);
  /** Seconds into the current leap. */
  private readonly fishLeapT = new Float32Array(MAX_FISH);
  private readonly fishFromX = new Float32Array(MAX_FISH);
  private readonly fishFromZ = new Float32Array(MAX_FISH);
  private readonly fishToX = new Float32Array(MAX_FISH);
  private readonly fishToZ = new Float32Array(MAX_FISH);
  private readonly fishBow = new Float32Array(MAX_FISH);
  /** Whether the current leap clears the weir (1) or falls short (0). */
  private readonly fishClears = new Uint8Array(MAX_FISH);
  /** Seconds spent waiting below the weir, and how long this salmon will wait before leaping. */
  readonly fishWait = new Float32Array(MAX_FISH);
  private readonly fishPatience = new Float32Array(MAX_FISH);
  private readonly fishWander = new Float32Array(MAX_FISH);
  private readonly fishRank = new Int32Array(MAX_FISH);
  /** Front waiting salmon per pool, cleared and filled each step with no allocation. */
  private readonly poolLeaper = new Int32Array(MAX_POOLS);

  /** Current glints drifting on the pools: position and the pool each stays in. */
  glintCount = 0;
  readonly glintX = new Float32Array(MAX_GLINTS);
  readonly glintY = new Float32Array(MAX_GLINTS);
  readonly glintPool = new Int32Array(MAX_GLINTS);

  readonly rippleX = new Float32Array(MAX_RIPPLES);
  readonly rippleY = new Float32Array(MAX_RIPPLES);
  readonly rippleZ = new Float32Array(MAX_RIPPLES);
  /** Seconds since the ripple started. */
  readonly rippleAge = new Float32Array(MAX_RIPPLES);
  /** Starting brightness in (0, 1]; 0 marks a free slot. */
  readonly rippleStrength = new Float32Array(MAX_RIPPLES);

  /** The leaf's position; `leafActive` is false when none is drifting. */
  leafActive = false;
  leafX = 0;
  leafY = 0;
  leafZ = 0;

  /** Phase of the water spilling down the falls, in cells; holds still under reduced motion. */
  fallPhase = 0;
  /** Reduced-motion light shift on the water from beats, in [0, 1]. */
  glow = 0;
  /** Rare voxel bear head popping up at a weir to catch a leaping salmon. */
  bearActive = false;
  bearX = 0;
  bearY = 0;
  bearZ = 0;
  /** How far the bear has risen above the water surface, in [0, 1]. */
  bearRise = 0;
  /** Jaw openness in [0, 1]; 1 while waiting for the salmon, 0 once it snaps shut. */
  bearJaw = 0;
  /** Bumped whenever the ladder is laid out again, so a cached drawing of it can tell it's stale. */
  layoutVersion = 0;

  private key: string | null = null;
  private readonly rng = new SeededRandom(0);
  private bearTarget = -1;
  private bearCaught = false;
  private bearTimer = 0;
  private bearCooldown = 3;

  constructor() {
    this.layOut("");
  }

  /**
   * Shows the ladder for the track `key`. A new key lays out a new ladder and school from a seed
   * of the key (the same key always gives the same ladder) and, after the first track, sets a
   * leaf floating down it. The same key again is a no-op.
   */
  setTrack(key: string): void {
    if (key === this.key) return;
    const first = this.key === null;
    this.key = key;
    this.layOut(key);
    if (!first) {
      this.leafActive = true;
      this.leafX = -1;
      this.leafY = this.notchCenter(0);
      this.leafZ = this.poolSurface[0];
    }
  }

  push(frame: VisualizerFrame): void {
    this.audio.push(frame);
  }

  /**
   * Advances the ladder by `dtMs`. Returns true while anything is still moving: always while
   * playing, and while paused until the river has calmed and every ripple, leap and the leaf
   * are done.
   */
  step(dtMs: number, reducedMotion: boolean): boolean {
    const dt = Math.max(0, Math.min(100, dtMs)) / 1000;
    const audio = this.audio;
    audio.step(dt);
    const flow = FLOW_SPEED * audio.activity;
    if (!reducedMotion) this.fallPhase = (this.fallPhase + flow * 2 * dt) % 64;

    if (reducedMotion) {
      if (audio.onset > 0) this.glow = Math.max(this.glow, audio.onset * GLOW_PER_ONSET);
      this.topBearNodT = 0;
      this.topBearNod = 0;
      this.topBearBreath = 0;
      this.topBearLookY = 0;
    } else {
      if (audio.onset > 0) {
        this.rippleOnBeat(audio.onset);
      }
      if (audio.activity > 0) {
        this.topBearIdle = (this.topBearIdle + dt * 1.4 * audio.activity) % (Math.PI * 2);
        this.topBearBreath = (0.5 - 0.5 * Math.cos(this.topBearIdle * 2)) * 0.14 * audio.activity;
        this.topBearLookY = Math.sin(this.topBearIdle) * 0.18 * audio.activity;
      } else {
        this.topBearBreath = 0;
        this.topBearLookY = 0;
      }
      if (this.topBearNodT > 0) {
        this.topBearNodT += dt;
        const u = this.topBearNodT / TOP_BEAR_NOD_S;
        if (u >= 1) {
          this.topBearNodT = 0;
          this.topBearNod = 0;
        } else {
          this.topBearNod = Math.sin(Math.PI * u);
        }
      }
    }
    this.stepFrog(dt, reducedMotion);
    this.glow = this.glow > 0.002 ? Math.max(0, this.glow - GLOW_DECAY * dt) : 0;

    this.moveSchool(dt, reducedMotion);
    this.stepBear(dt, reducedMotion);
    this.moveGlints(flow * dt);
    const rippling = this.moveRipples(dt, reducedMotion);
    this.moveLeaf(dt, reducedMotion);

    let leaping = false;
    for (let i = 0; i < this.fishCount; i++) if (this.fishLeapS[i] > 0) leaping = true;
    return (
      audio.stirring ||
      rippling ||
      leaping ||
      this.leafActive ||
      this.bearActive ||
      this.frogJumpT > 0 ||
      this.frogWaterT > 0 ||
      this.glow > 0
    );
  }

  /** Middle of notch `n` in pool k's weir, in y. */
  notchCenter(k: number, n = 0): number {
    const slot = k * MAX_NOTCHES + n;
    return this.notchY[slot] + this.notchW[slot] / 2;
  }

  /** True when cell `y` lies inside any overflow notch of pool k's weir. */
  isInNotch(k: number, y: number): boolean {
    const base = k * MAX_NOTCHES;
    const count = this.notchCount[k];
    for (let n = 0; n < count; n++) {
      const y0 = this.notchY[base + n];
      if (y >= y0 && y < y0 + this.notchW[base + n]) return true;
    }
    return false;
  }

  /** Center y of the notch in pool k's weir closest to `y`. */
  nearestNotchCenter(k: number, y: number): number {
    const count = this.notchCount[k];
    let best = this.notchCenter(k, 0);
    let bestDist = Math.abs(y - best);
    for (let n = 1; n < count; n++) {
      const c = this.notchCenter(k, n);
      const d = Math.abs(y - c);
      if (d < bestDist) {
        best = c;
        bestDist = d;
      }
    }
    return best;
  }

  /**
   * Downstream end of pool k's open water: its weir's row for every pool but the bottom one,
   * whose water runs to the front edge.
   */
  openWaterEnd(k: number): number {
    return this.poolEnd[k] - (k < this.poolCount - 1 ? 1 : 0);
  }

  /** The pool whose water lies under `x`: the top pool upstream of the ladder, the bottom beyond it. */
  poolAt(x: number): number {
    for (let k = 0; k < this.poolCount - 1; k++) if (x < this.poolEnd[k]) return k;
    return this.poolCount - 1;
  }

  /** Radius of ripple `i`, in cells. */
  rippleRadius(i: number): number {
    return 0.3 + RIPPLE_GROWTH * this.rippleAge[i];
  }

  /** Brightness in [0, 1] of ripple `i`, fading over its life. */
  rippleAlpha(i: number): number {
    const s = this.rippleStrength[i];
    return s === 0 ? 0 : s * (1 - this.rippleAge[i] / RIPPLE_LIFE_S);
  }

  private spawnRipple(x: number, y: number, z: number, strength: number): void {
    let slot = 0;
    for (let i = 0; i < MAX_RIPPLES; i++) {
      if (this.rippleStrength[i] === 0) {
        slot = i;
        break;
      }
      if (this.rippleAge[i] > this.rippleAge[slot]) slot = i;
    }
    this.rippleX[slot] = x;
    this.rippleY[slot] = y;
    this.rippleZ[slot] = z;
    this.rippleAge[slot] = 0;
    this.rippleStrength[slot] = Math.max(0.05, Math.min(1, strength));
  }

  /** A beat's ripple: at the foot of a fall, or anywhere on a pool when the ladder is one pool. */
  private rippleOnBeat(onset: number): void {
    const strength = 0.4 + 0.6 * onset;
    if (this.poolCount > 1) {
      const k = this.rng.int(1, this.poolCount - 1);
      const n = this.rng.int(0, Math.max(0, this.notchCount[k - 1] - 1));
      const y = this.notchCenter(k - 1, n) + this.rng.range(-1, 1);
      this.spawnRipple(this.poolStart[k] + 1, y, this.poolSurface[k], strength);
    } else {
      const x = this.rng.range(this.poolStart[0] + 1, this.poolEnd[0] - 1);
      this.spawnRipple(x, this.rng.range(1, RIVER_WIDTH - 1), this.poolSurface[0], strength);
    }
  }

  /** Where a salmon in pool k heads: just below the step it will climb, or off the top. */
  private spotX(k: number): number {
    return k === 0 ? -2.5 : this.poolStart[k] + WAIT_DEPTH;
  }

  private spotY(k: number, nearY = RIVER_WIDTH / 2): number {
    return k === 0 ? RIVER_WIDTH / 2 : this.nearestNotchCenter(k - 1, nearY);
  }

  private moveSchool(dt: number, reducedMotion: boolean): void {
    const n = this.fishCount;
    const audio = this.audio;
    const a = audio.activity;
    const pace = reducedMotion ? REDUCED_PACE : 1;
    const speed = (SWIM_BASE + SWIM_ENERGY * audio.energy) * a * pace;

    // Rank the swimmers in each pool by how near they are to its step: the front one waits there,
    // the rest queue behind it.
    for (let i = 0; i < n; i++) {
      let rank = 0;
      if (this.fishLeapS[i] === 0) {
        for (let j = 0; j < n; j++) {
          if (j === i || this.fishLeapS[j] > 0 || this.fishPool[j] !== this.fishPool[i]) continue;
          if (this.fishX[j] < this.fishX[i] || (this.fishX[j] === this.fishX[i] && j < i)) rank++;
        }
      }
      this.fishRank[i] = rank;
    }

    this.poolLeaper.fill(-1, 0, this.poolCount);
    for (let i = 0; i < n; i++) {
      if (this.fishLeapS[i] > 0) {
        // Under reduced motion nothing is shown in flight: a leap already under way (the one a
        // new layout starts with) lands at once.
        this.flyLeap(i, reducedMotion ? this.fishLeapS[i] : dt);
        continue;
      }
      const k = this.fishPool[i];
      const rank = this.fishRank[i];
      this.fishWander[i] += dt * a * 0.6;
      const w = this.fishWander[i];
      let targetX: number;
      let targetY: number;
      const aimY = this.spotY(k, this.fishY[i]);
      if (rank === 0) {
        targetX = this.spotX(k);
        targetY = aimY + Math.sin(w * 1.3) * 0.4;
      } else {
        // Fan queued salmon laterally across the pool instead of stacking them single-file at the weir.
        const loX = k > 0 ? this.poolStart[k] + STEP_CLEARANCE : this.poolStart[k] + 0.5;
        const hiX =
          k < this.poolCount - 1 ? this.poolEnd[k] - WEIR_CLEARANCE : this.poolEnd[k] - 0.5;
        const lane = (rank + 1) >> 1;
        const side = rank % 2 === 1 ? -1 : 1;
        const spreadY = Math.min(RIVER_WIDTH * 0.42, lane * 2.1);
        const row = Math.floor((rank - 1) / 3);
        targetX = Math.min(hiX, loX + 1.2 + row * 1.4 + Math.sin(w) * 0.4);
        targetY = Math.max(
          BANK_CLEARANCE + 0.4,
          Math.min(
            RIVER_WIDTH - BANK_CLEARANCE - 0.4,
            aimY + side * spreadY + Math.sin(w * 1.3) * 0.6,
          ),
        );
      }

      let steerX = (targetX - this.fishX[i]) * 0.9;
      let steerY = (targetY - this.fishY[i]) * 0.9;
      for (let j = 0; j < n; j++) {
        if (j === i || this.fishLeapS[j] > 0 || this.fishPool[j] !== k) continue;
        const dx = this.fishX[i] - this.fishX[j];
        const dy = this.fishY[i] - this.fishY[j];
        const d = Math.hypot(dx, dy);
        if (d > 0 && d < 2.0) {
          steerX += (dx / d) * (2.0 - d);
          steerY += (dy / d) * (2.0 - d) * 2.0;
        }
      }
      for (let b = 0; b < this.boulderCount; b++) {
        if (this.boulderPool[b] !== k) continue;
        const dx = this.fishX[i] - (this.boulderX[b] + 0.5);
        const dy = this.fishY[i] - (this.boulderY[b] + 0.5);
        const d = Math.hypot(dx, dy);
        if (d > 0 && d < 1.8) steerY += (dy / d) * (1.8 - d) * 2.5;
      }
      const mag = Math.hypot(steerX, steerY);
      const limit = mag > speed && mag > 0 ? speed / mag : 1;
      const ease = 1 - Math.exp(-3 * dt);
      this.fishVX[i] += (steerX * limit * a - this.fishVX[i]) * ease;
      this.fishVY[i] += (steerY * limit * a - this.fishVY[i]) * ease;

      let x = this.fishX[i] + this.fishVX[i] * dt;
      // A salmon can't swim up a step, only leap it; nor swim through its weir.
      if (k > 0) x = Math.max(this.poolStart[k] + STEP_CLEARANCE, x);
      if (k < this.poolCount - 1) x = Math.min(this.poolEnd[k] - WEIR_CLEARANCE, x);
      this.fishX[i] = x;
      this.fishY[i] = Math.max(
        BANK_CLEARANCE,
        Math.min(RIVER_WIDTH - BANK_CLEARANCE, this.fishY[i] + this.fishVY[i] * dt),
      );
      this.fishZ[i] = this.poolSurface[k];
      this.fishPitch[i] = 0;

      const hz = (TAIL_BASE_HZ + TAIL_ENERGY_HZ * audio.energy) * a * pace;
      this.fishTail[i] = (this.fishTail[i] + hz * dt * Math.PI * 2) % (Math.PI * 2);

      // Eaten by the sitting bear in the top pool if within its reach, or out of the top pool.
      const eatenByTopBear =
        !reducedMotion &&
        k === 0 &&
        Math.abs(x - this.topBearX) < 2.2 &&
        Math.abs(this.fishY[i] - this.topBearY) < 2.2;
      if (eatenByTopBear) {
        this.spawnRipple(this.topBearX + 0.8, this.topBearY, this.poolSurface[0], 0.9);
        this.topBearNodT = Math.max(this.topBearNodT, dt);
        this.topBearNod = Math.sin(Math.PI * Math.min(1, this.topBearNodT / TOP_BEAR_NOD_S));
      }
      if (eatenByTopBear || (k === 0 && x < -1.5)) {
        this.resetFishToBottom(i);
        continue;
      }

      const ready =
        k > 0 &&
        rank === 0 &&
        Math.abs(x - this.spotX(k)) < READY_REACH &&
        Math.abs(this.fishY[i] - aimY) < READY_REACH * 1.5;
      if (!ready || reducedMotion) {
        this.fishWait[i] = 0;
        continue;
      }
      if (audio.playing) this.fishWait[i] += dt * a;
      if (this.fishWait[i] >= this.fishPatience[i]) this.startLeap(i, true);
      else {
        const cur = this.poolLeaper[k];
        if (cur < 0 || this.fishWait[i] > this.fishWait[cur]) this.poolLeaper[k] = i;
      }
    }

    // A transient sends the waiting salmon in every pool up its step, if it's strong enough.
    if (!reducedMotion && audio.transient > 0) {
      const clears = audio.transient >= CLEAR_STRENGTH;
      for (let k = 1; k < this.poolCount; k++) {
        const i = this.poolLeaper[k];
        if (i >= 0) this.startLeap(i, clears);
      }
    }
  }

  /**
   * Starts salmon `i` leaping from where it waits: up a step, or (`clears` false) falling short.
   * Only salmon below a step wait to leap; the top pool's swim off upstream instead.
   */
  private startLeap(i: number, clears: boolean, fromLayout = false): void {
    const k = this.fishPool[i];
    const x = this.fishX[i];
    const z = this.poolSurface[k];
    this.fishFromX[i] = x;
    this.fishFromZ[i] = z;
    this.fishClears[i] = clears ? 1 : 0;
    if (clears) {
      this.fishToX[i] = this.poolEnd[k - 1] - WEIR_CLEARANCE - 0.2;
      this.fishToZ[i] = this.poolSurface[k - 1];
      this.fishBow[i] = LEAP_BOW;
      this.fishLeapS[i] = LEAP_S;
      if (
        !fromLayout &&
        !this.bearActive &&
        this.bearCooldown === 0 &&
        this.audio.playing &&
        this.rng.next() < BEAR_CHANCE
      ) {
        this.spawnBear(i, k - 1);
      }
    } else {
      this.fishToX[i] = x + 0.7;
      this.fishToZ[i] = z;
      this.fishBow[i] = SHORT_LEAP_BOW;
      this.fishLeapS[i] = SHORT_LEAP_S;
    }
    this.fishLeapT[i] = 0;
    this.fishWait[i] = 0;
    this.fishVX[i] = this.fishVY[i] = 0;
    this.spawnRipple(x, this.fishY[i], z, 0.8);
  }

  /** Carries salmon `i` along its leap's arc, landing it (and splashing) at the end. */
  private flyLeap(i: number, dt: number): void {
    const total = this.fishLeapS[i];
    const t = Math.min(total, this.fishLeapT[i] + dt);
    this.fishLeapT[i] = t;
    const u = t / total;
    const dx = this.fishToX[i] - this.fishFromX[i];
    const dz = this.fishToZ[i] - this.fishFromZ[i];
    const bow = this.fishBow[i];
    this.fishX[i] = this.fishFromX[i] + dx * u;
    this.fishZ[i] = this.fishFromZ[i] + dz * u + bow * Math.sin(Math.PI * u);
    // The body follows the arc, nose up on the way up: slope of height over ground covered.
    const rise = dz + bow * Math.PI * Math.cos(Math.PI * u);
    this.fishPitch[i] = Math.max(-1.2, Math.min(1.2, rise / Math.max(0.5, Math.abs(dx))));
    this.fishTail[i] = (this.fishTail[i] + 9 * dt * Math.PI * 2) % (Math.PI * 2);
    if (t < total) return;

    this.fishLeapS[i] = this.fishLeapT[i] = 0;
    this.fishPitch[i] = 0;
    if (this.fishClears[i]) this.fishPool[i]--;
    const k = this.fishPool[i];
    this.fishZ[i] = this.poolSurface[k];
    this.fishVX[i] = -0.3;
    this.fishPatience[i] = this.rng.range(PATIENCE_S[0], PATIENCE_S[1]);
    this.spawnRipple(this.fishX[i], this.fishY[i], this.poolSurface[k], 1);
  }

  /** Makes the frog jump into the water (or back onto its boulder). Returns true if started. */
  triggerFrogJump(): boolean {
    if (this.frogBoulder < 0 || this.frogJumpT > 0) return false;
    const b = this.frogBoulder;
    const k = this.boulderPool[b];
    const bx = this.boulderX[b] + 0.5;
    const by = this.boulderY[b] + 0.5;
    const bz = this.poolSurface[k] + 1;
    this.frogFromX = this.frogX;
    this.frogFromY = this.frogY;
    this.frogFromZ = this.frogZ;
    if (!this.frogInWater) {
      this.frogToX = Math.min(this.openWaterEnd(k) - 0.6, bx + 0.8);
      this.frogToY = by < RIVER_WIDTH / 2 ? by + 2.4 : by - 2.4;
      this.frogToZ = this.poolSurface[k];
      this.frogInWater = true;
      this.frogWaterT = 0;
    } else {
      this.frogToX = bx;
      this.frogToY = by;
      this.frogToZ = bz;
      this.frogInWater = false;
      this.frogWaterT = 0;
    }
    this.frogJumpT = 0.001;
    return true;
  }

  private placeFrogOnBoulder(): void {
    const b = this.frogBoulder;
    this.frogX = this.boulderX[b] + 0.5;
    this.frogY = this.boulderY[b] + 0.5;
    this.frogZ = this.poolSurface[this.boulderPool[b]] + 1;
    this.frogJumpT = 0;
    this.frogWaterT = 0;
    this.frogInWater = false;
  }

  private stepFrog(dt: number, reducedMotion: boolean): void {
    if (this.frogBoulder < 0) return;
    if (reducedMotion) {
      this.placeFrogOnBoulder();
      return;
    }
    if (this.frogJumpT > 0) {
      this.frogJumpT += dt;
      const u = Math.min(1, this.frogJumpT / FROG_JUMP_S);
      this.frogX = this.frogFromX + (this.frogToX - this.frogFromX) * u;
      this.frogY = this.frogFromY + (this.frogToY - this.frogFromY) * u;
      this.frogZ =
        this.frogFromZ +
        (this.frogToZ - this.frogFromZ) * u +
        FROG_JUMP_BOW * Math.sin(Math.PI * u);
      if (u >= 1) {
        this.frogJumpT = 0;
        this.frogX = this.frogToX;
        this.frogY = this.frogToY;
        this.frogZ = this.frogToZ;
        if (this.frogInWater) {
          this.spawnRipple(this.frogX, this.frogY, this.frogZ, 0.85);
          this.frogWaterT = FROG_WATER_S;
        }
      }
      return;
    }
    if (this.frogInWater && this.frogWaterT > 0) {
      this.frogWaterT = Math.max(0, this.frogWaterT - dt);
      if (this.frogWaterT === 0) {
        this.triggerFrogJump();
      }
    }
  }

  /** Pops up the voxel bear head at the weir of `upperPool` to intercept leaping salmon `i`. */
  spawnBear(i: number, upperPool: number): void {
    this.bearActive = true;
    this.bearTarget = i;
    this.bearCaught = false;
    this.bearTimer = 0;
    this.bearX = this.poolEnd[upperPool] - 0.8;
    this.bearY = this.fishY[i];
    this.bearZ = this.poolSurface[upperPool];
    this.bearRise = 0;
    this.bearJaw = 1;
    this.bearCooldown = BEAR_COOLDOWN_S;
  }

  private stepBear(dt: number, reducedMotion: boolean): void {
    this.bearCooldown = Math.max(0, this.bearCooldown - dt);
    if (reducedMotion) {
      this.bearActive = false;
      this.bearTarget = -1;
      return;
    }
    if (!this.bearActive) return;

    if (!this.bearCaught) {
      const i = this.bearTarget;
      const total = i >= 0 ? this.fishLeapS[i] : 0;
      if (total > 0) {
        const u = this.fishLeapT[i] / total;
        this.bearRise = Math.min(1, u / 0.45);
        this.bearJaw = 1;
        if (u >= 0.62) {
          this.bearCaught = true;
          this.bearJaw = 0;
          this.bearTimer = 0;
          // Salmon is caught and rejoins the run below the bottom pool.
          this.resetFishToBottom(i);
          this.spawnRipple(this.bearX + 0.5, this.bearY, this.bearZ, 1);
        }
      } else {
        this.bearCaught = true;
        this.bearJaw = 0;
        this.bearTimer = 0;
      }
      return;
    }

    this.bearTimer += dt;
    if (this.bearTimer < BEAR_HOLD_S) {
      this.bearRise = 1;
    } else {
      const sinkU = (this.bearTimer - BEAR_HOLD_S) / BEAR_SINK_S;
      if (sinkU >= 1) {
        this.bearActive = false;
        this.bearRise = 0;
        this.bearTarget = -1;
      } else {
        this.bearRise = 1 - sinkU;
      }
    }
  }

  private resetFishToBottom(i: number): void {
    const bottom = this.poolCount - 1;
    this.fishLeapS[i] = this.fishLeapT[i] = 0;
    this.fishPitch[i] = 0;
    this.fishPool[i] = bottom;
    this.fishX[i] = this.length + this.rng.range(1.5, 3);
    this.fishY[i] = this.rng.range(2, RIVER_WIDTH - 2);
    this.fishZ[i] = this.poolSurface[bottom];
    this.fishVX[i] = this.fishVY[i] = 0;
    this.fishWait[i] = 0;
  }

  private moveGlints(dx: number): void {
    for (let g = 0; g < this.glintCount; g++) {
      const k = this.glintPool[g];
      // Faster mid-channel than near the banks.
      const lane = 1 - Math.abs(this.glintY[g] / RIVER_WIDTH - 0.5);
      this.glintX[g] += dx * lane;
      const end = this.openWaterEnd(k) - 0.5;
      if (this.glintX[g] > end) this.glintX[g] = this.poolStart[k] + 0.2;
    }
  }

  private moveRipples(dt: number, reducedMotion: boolean): boolean {
    let any = false;
    for (let i = 0; i < MAX_RIPPLES; i++) {
      if (this.rippleStrength[i] === 0) continue;
      this.rippleAge[i] += dt;
      if (reducedMotion || this.rippleAge[i] >= RIPPLE_LIFE_S) this.rippleStrength[i] = 0;
      else any = true;
    }
    return any;
  }

  /**
   * Floats the leaf downstream, steering it toward each weir's notch and dropping it down the
   * falls below.
   */
  private moveLeaf(dt: number, reducedMotion: boolean): void {
    if (!this.leafActive) return;
    const speed = (this.length + 2) / LEAF_CROSS_S;
    const x = this.leafX + speed * dt * (reducedMotion ? LEAF_REDUCED_PACE : 1);
    this.leafX = x;
    const k = this.poolAt(x);
    const aim = k < this.poolCount - 1 ? this.nearestNotchCenter(k, this.leafY) : RIVER_WIDTH / 2;
    this.leafY += (aim - this.leafY) * Math.min(1, dt * 0.8);
    // Over the lip it falls the step's height in its first cell.
    const past = k > 0 ? x - this.poolStart[k] : 1;
    const fall = past < 1 ? 1 - past * past : 0;
    this.leafZ = this.poolSurface[k] + (k > 0 ? DROP * fall : 0);
    if (x > this.length + 1) this.leafActive = false;
  }

  /** Lays out the ladder, glints and school from the seed of `key`. */
  private layOut(key: string): void {
    const rng = this.rng;
    rng.reseed(seedFromKey(key));
    this.layoutVersion++;

    const w = RIVER_WIDTH;
    const pools = MAX_POOLS;
    this.poolCount = pools;
    let x = 0;
    for (let k = 0; k < pools; k++) {
      this.poolStart[k] = x;
      x += k === 0 ? rng.int(12, 16) : rng.int(7, 9);
      this.poolEnd[k] = x;
      this.poolSurface[k] = BED_DEPTH + WATER_DEPTH + (pools - 1 - k) * DROP;
      const maxNotches = Math.min(MAX_NOTCHES, Math.max(1, Math.floor((w - 2) / 3)));
      const count = rng.int(1, maxNotches);
      this.notchCount[k] = count;
      const span = (w - 2) / count;
      let prevEnd = 0;
      for (let n = 0; n < count; n++) {
        const nw = count === 1 ? Math.min(NOTCH_WIDTH, w - 2) : 2;
        const lo = Math.max(prevEnd + 1, Math.floor(1 + n * span));
        const hi = Math.min(
          w - nw - 1,
          Math.floor(1 + (n + 1) * span) - nw - (n < count - 1 ? 1 : 0),
        );
        const ny = hi >= lo ? rng.int(lo, hi) : lo;
        const slot = k * MAX_NOTCHES + n;
        this.notchY[slot] = ny;
        this.notchW[slot] = nw;
        prevEnd = ny + nw;
      }
    }
    this.length = x;

    // A boulder in some pools, clear of the approach to the step and of the weir.
    this.boulderCount = 0;
    for (let k = 0; k < pools; k++) {
      if (rng.next() < 0.35) continue;
      const bx = rng.int(this.poolStart[k] + 3, this.poolEnd[k] - 3);
      const by = rng.next() < 0.5 ? rng.int(0, 1) : rng.int(w - 2, w - 1);
      const b = this.boulderCount++;
      this.boulderX[b] = bx;
      this.boulderY[b] = by;
      this.boulderPool[b] = k;
    }
    this.frogBoulder = this.boulderCount > 0 ? rng.int(0, this.boulderCount - 1) : -1;
    if (this.frogBoulder >= 0) {
      this.placeFrogOnBoulder();
    } else {
      this.frogInWater = false;
      this.frogJumpT = 0;
      this.frogWaterT = 0;
    }
    this.topBearX = (this.poolStart[0] + this.poolEnd[0]) * 0.5;
    this.topBearY = w * 0.5;
    this.topBearNodT = 0;
    this.topBearNod = 0;
    this.topBearBreath = 0;
    this.topBearLookY = 0;
    this.topBearIdle = 0;

    // Sparse vegetation along the far bank: pines on the back ridge, low shrubs near the water.
    this.bankPlant.fill(BankPlant.NONE, 0, this.length * BANK_WIDTH);
    let lastTree = -3;
    for (let x = 0; x < this.length; x++) {
      if (x - lastTree >= 3 && rng.next() < 0.28) {
        this.bankPlant[x * BANK_WIDTH] = BankPlant.PINE;
        lastTree = x;
      } else if (rng.next() < 0.22) {
        this.bankPlant[x * BANK_WIDTH] =
          rng.next() < 0.35 ? BankPlant.AUTUMN_BUSH : BankPlant.SHRUB;
      }
      if (rng.next() < 0.2) {
        this.bankPlant[x * BANK_WIDTH + 1] =
          rng.next() < 0.3 ? BankPlant.AUTUMN_BUSH : BankPlant.SHRUB;
      }
    }

    this.glintCount = pools * GLINTS_PER_POOL;
    for (let g = 0; g < this.glintCount; g++) {
      const k = Math.floor(g / GLINTS_PER_POOL);
      this.glintPool[g] = k;
      this.glintX[g] = rng.range(this.poolStart[k] + 0.2, this.poolEnd[k] - 1.5);
      this.glintY[g] = rng.range(0.5, w - 0.5);
    }

    // The school is spread up the ladder, and the first frame already shows the climb: one salmon
    // in mid-leap up a step near the middle and one waiting below the step after it.
    this.fishCount = MAX_FISH;
    const middle = Math.max(1, Math.floor(pools / 2));
    for (let i = 0; i < this.fishCount; i++) {
      const k = i < 2 ? Math.min(pools - 1, middle + i) : rng.int(0, pools - 1);
      this.fishPool[i] = k;
      this.fishX[i] = i < 2 ? this.spotX(k) : rng.range(this.poolStart[k] + 1, this.poolEnd[k] - 1);
      this.fishY[i] = i < 2 ? this.spotY(k) : rng.range(1, w - 1);
      this.fishZ[i] = this.poolSurface[k];
      this.fishVX[i] = this.fishVY[i] = this.fishPitch[i] = 0;
      this.fishTail[i] = rng.range(0, Math.PI * 2);
      this.fishScale[i] = rng.range(0.85, 1.12);
      this.fishLeapS[i] = this.fishLeapT[i] = 0;
      this.fishWait[i] = 0;
      this.fishPatience[i] = rng.range(PATIENCE_S[0], PATIENCE_S[1]);
      this.fishWander[i] = rng.range(0, Math.PI * 2);
    }
    // Clamp the scattered ones into their pools' swimming room (and clear of the top-pool bear).
    for (let i = 2; i < this.fishCount; i++) {
      const k = this.fishPool[i];
      const lo = k > 0 ? this.poolStart[k] + STEP_CLEARANCE : this.poolStart[k] + 0.5;
      const hi = k < pools - 1 ? this.poolEnd[k] - WEIR_CLEARANCE : this.poolEnd[k] - 0.5;
      this.fishX[i] = Math.max(lo, Math.min(hi, this.fishX[i]));
      if (
        k === 0 &&
        Math.abs(this.fishX[i] - this.topBearX) < 2.5 &&
        Math.abs(this.fishY[i] - this.topBearY) < 2.5
      ) {
        this.fishX[i] = Math.max(lo, this.poolEnd[0] - 2);
      }
    }
    this.startLeap(0, true, true);
    this.fishLeapT[0] = LEAP_S * 0.45;
    this.flyLeap(0, 0);
    this.bearActive = false;
    this.bearTarget = -1;
    this.bearCaught = false;
    this.bearTimer = 0;
    this.bearRise = 0;
    this.bearJaw = 0;
    this.bearCooldown = 3;
    for (let i = 0; i < MAX_RIPPLES; i++) this.rippleStrength[i] = 0;
  }
}
