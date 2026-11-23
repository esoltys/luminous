import { RiverAudio, SeededRandom, seedFromKey } from "./salmonRunEngine";
import type { VisualizerFrame } from "./types";

// The fish ladder is laid out in scene units, one per terrain cube: x runs downstream along the
// river (from the top pool at the back, x = 0, to the front edge at `length`), y across it (0 at
// the far bank to `RIVER_WIDTH` at the near side) and z up. The ladder steps down toward the
// viewer, so a pool sits `DROP` above the one in front of it. Nothing here depends on the canvas:
// the renderer scales the whole scene to fit.

/** Water cells across the river. */
export const RIVER_WIDTH = 9;
/** Cells of bank along the far side, at y in [-BANK_WIDTH, 0). */
export const BANK_WIDTH = 2;
/** Height of each step, from one pool's surface to the next pool's. */
export const DROP = 3;
/** Cubes of water in a pool, and of stone under the bottom pool's water. */
export const WATER_DEPTH = 2;
const BED_DEPTH = 1;
/** Width of the notch in each weir that the water spills through. */
export const NOTCH_WIDTH = 3;
/** Most pools, boulders, fish and current glints any ladder holds. */
export const MAX_POOLS = 6;
const MAX_BOULDERS = MAX_POOLS;
export const MAX_FISH = 9;
const GLINTS_PER_POOL = 3;
const MAX_GLINTS = MAX_POOLS * GLINTS_PER_POOL;
/** Ripples in flight at once; a new one replaces the oldest beyond this. */
export const RIPPLE_POOL = 12;
const RIPPLE_LIFE_S = 1.4;
const RIPPLE_GROWTH = 1.7;

/** Seconds a leap lasts, and how high its arc bows above the straight line from take-off. */
export const LEAP_S = 0.9;
const LEAP_BOW = 2.6;
/** A short leap: it bows below the lip and drops back into the pool it left. */
const SHORT_LEAP_S = 0.7;
const SHORT_LEAP_BOW = 1.8;
/** Transient strength a leap needs to clear the weir; weaker ones fall short. */
const CLEAR_STRENGTH = 0.7;
/** Seconds a salmon waits below a weir before it leaps on its own, drawn per wait. */
export const PATIENCE_S: readonly [number, number] = [4, 8];

/** Speed of the surface current at full activity, in cells per second. */
const FLOW_SPEED = 1.1;
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
/** Spacing of the queue behind the salmon at the front. */
const QUEUE_GAP = 2.8;
/** How close to its waiting spot a salmon must be to leap. */
const READY_REACH = 1.2;
/** Clearance a salmon keeps from the step above it, from its weir, and from the banks. */
const STEP_CLEARANCE = 2.3;
const WEIR_CLEARANCE = 3;
const BANK_CLEARANCE = 0.7;
/** Reduced-motion "light shift" a beat leaves on the water, and its decay per second. */
const GLOW_PER_ONSET = 0.6;
const GLOW_DECAY = 1.2;

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
   * First y cell of the notch in pool k's weir (its last row, x = end - 1), `NOTCH_WIDTH` wide.
   * The bottom pool has no weir.
   */
  readonly notchY = new Int32Array(MAX_POOLS);
  /** Downstream end of the whole ladder. */
  length = 0;

  /** Boulders, one cell each, standing a cube above their pool's surface. */
  boulderCount = 0;
  readonly boulderX = new Int32Array(MAX_BOULDERS);
  readonly boulderY = new Int32Array(MAX_BOULDERS);
  readonly boulderPool = new Int32Array(MAX_BOULDERS);

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
  /** Seconds into the current leap, and its length; `fishLeapS` is 0 while swimming. */
  readonly fishLeapT = new Float32Array(MAX_FISH);
  readonly fishLeapS = new Float32Array(MAX_FISH);
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

  /** Current glints drifting on the pools: position and the pool each stays in. */
  glintCount = 0;
  readonly glintX = new Float32Array(MAX_GLINTS);
  readonly glintY = new Float32Array(MAX_GLINTS);
  readonly glintPool = new Int32Array(MAX_GLINTS);

  readonly rippleX = new Float32Array(RIPPLE_POOL);
  readonly rippleY = new Float32Array(RIPPLE_POOL);
  readonly rippleZ = new Float32Array(RIPPLE_POOL);
  /** Seconds since the ripple started. */
  readonly rippleAge = new Float32Array(RIPPLE_POOL);
  /** Starting brightness in (0, 1]; 0 marks a free slot. */
  readonly rippleStrength = new Float32Array(RIPPLE_POOL);

  /** The leaf's position; `leafActive` is false when none is drifting. */
  leafActive = false;
  leafX = 0;
  leafY = 0;
  leafZ = 0;

  /** Phase of the water spilling down the falls, in cells; holds still under reduced motion. */
  fallPhase = 0;
  /** Reduced-motion light shift on the water from beats, in [0, 1]. */
  glow = 0;
  /** Bumped whenever the ladder is laid out again, so a cached drawing of it can tell it's stale. */
  layoutVersion = 0;

  private key: string | null = null;
  private readonly rng = new SeededRandom(0);

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
    } else if (audio.onset > 0) {
      this.rippleOnBeat(audio.onset);
    }
    this.glow = this.glow > 0.002 ? Math.max(0, this.glow - GLOW_DECAY * dt) : 0;

    this.moveSchool(dt, reducedMotion);
    this.moveGlints(flow * dt);
    const rippling = this.moveRipples(dt, reducedMotion);
    this.moveLeaf(dt, reducedMotion);

    let leaping = false;
    for (let i = 0; i < this.fishCount; i++) if (this.fishLeapS[i] > 0) leaping = true;
    return audio.settling || rippling || leaping || this.leafActive || this.glow > 0;
  }

  /** Middle of the notch in pool k's weir, in y. */
  notchCenter(k: number): number {
    return this.notchY[k] + NOTCH_WIDTH / 2;
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
    for (let i = 0; i < RIPPLE_POOL; i++) {
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
      const y = this.notchCenter(k - 1) + this.rng.range(-1, 1);
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

  private spotY(k: number): number {
    return k === 0 ? RIVER_WIDTH / 2 : this.notchCenter(k - 1);
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

    let leaper = -1;
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
      const targetX = this.spotX(k) + rank * QUEUE_GAP + (rank > 0 ? Math.sin(w) * 0.5 : 0);
      const targetY = this.spotY(k) + Math.sin(w * 1.3) * (rank > 0 ? 1.8 : 0.4);

      let steerX = (targetX - this.fishX[i]) * 0.9;
      let steerY = (targetY - this.fishY[i]) * 0.9;
      for (let j = 0; j < n; j++) {
        if (j === i || this.fishLeapS[j] > 0 || this.fishPool[j] !== k) continue;
        const dx = this.fishX[i] - this.fishX[j];
        const dy = this.fishY[i] - this.fishY[j];
        const d = Math.hypot(dx, dy);
        if (d > 0 && d < 1.6) {
          steerX += (dx / d) * (1.6 - d);
          steerY += (dy / d) * (1.6 - d) * 1.5;
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

      // Out of the top pool: rejoin the run below the ladder, out of sight.
      if (k === 0 && x < -1.5) {
        this.fishPool[i] = this.poolCount - 1;
        this.fishX[i] = this.length + this.rng.range(1.5, 3);
        this.fishY[i] = this.rng.range(2, RIVER_WIDTH - 2);
        this.fishZ[i] = this.poolSurface[this.poolCount - 1];
        this.fishWait[i] = 0;
        continue;
      }

      const ready =
        k > 0 &&
        rank === 0 &&
        Math.abs(x - this.spotX(k)) < READY_REACH &&
        Math.abs(this.fishY[i] - this.spotY(k)) < READY_REACH * 1.5;
      if (!ready || reducedMotion) {
        this.fishWait[i] = 0;
        continue;
      }
      if (audio.playing) this.fishWait[i] += dt * a;
      if (this.fishWait[i] >= this.fishPatience[i]) this.startLeap(i, true);
      else if (leaper < 0 || this.fishWait[i] > this.fishWait[leaper]) leaper = i;
    }

    // A transient sends the salmon that has waited longest up its step, if it's strong enough.
    if (!reducedMotion && audio.transient > 0 && leaper >= 0) {
      this.startLeap(leaper, audio.transient >= CLEAR_STRENGTH);
    }
  }

  /** Starts salmon `i` leaping from where it waits: up a step, or (`clears` false) falling short. */
  private startLeap(i: number, clears: boolean): void {
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

  private moveGlints(dx: number): void {
    for (let g = 0; g < this.glintCount; g++) {
      const k = this.glintPool[g];
      // Faster mid-channel than near the banks.
      const lane = 1 - Math.abs(this.glintY[g] / RIVER_WIDTH - 0.5);
      this.glintX[g] += dx * lane;
      const end = this.poolEnd[k] - (k < this.poolCount - 1 ? 1.5 : 0.5);
      if (this.glintX[g] > end) this.glintX[g] = this.poolStart[k] + 0.2;
    }
  }

  private moveRipples(dt: number, reducedMotion: boolean): boolean {
    let any = false;
    for (let i = 0; i < RIPPLE_POOL; i++) {
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
    const aim = k < this.poolCount - 1 ? this.notchCenter(k) : RIVER_WIDTH / 2;
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

    const pools = rng.int(4, MAX_POOLS);
    this.poolCount = pools;
    let x = 0;
    for (let k = 0; k < pools; k++) {
      this.poolStart[k] = x;
      x += rng.int(7, 9);
      this.poolEnd[k] = x;
      this.poolSurface[k] = BED_DEPTH + WATER_DEPTH + (pools - 1 - k) * DROP;
      this.notchY[k] = rng.int(1, RIVER_WIDTH - NOTCH_WIDTH - 1);
    }
    this.length = x;

    // A boulder in some pools, clear of the approach to the step and of the weir.
    this.boulderCount = 0;
    for (let k = 0; k < pools; k++) {
      if (rng.next() < 0.35) continue;
      const bx = rng.int(this.poolStart[k] + 3, this.poolEnd[k] - 3);
      const by = rng.next() < 0.5 ? rng.int(0, 1) : rng.int(RIVER_WIDTH - 2, RIVER_WIDTH - 1);
      const b = this.boulderCount++;
      this.boulderX[b] = bx;
      this.boulderY[b] = by;
      this.boulderPool[b] = k;
    }

    this.glintCount = pools * GLINTS_PER_POOL;
    for (let g = 0; g < this.glintCount; g++) {
      const k = Math.floor(g / GLINTS_PER_POOL);
      this.glintPool[g] = k;
      this.glintX[g] = rng.range(this.poolStart[k] + 0.2, this.poolEnd[k] - 1.5);
      this.glintY[g] = rng.range(0.5, RIVER_WIDTH - 0.5);
    }

    // The school is spread up the ladder, and the first frame already shows the climb: one salmon
    // in mid-leap up a step near the middle and one waiting below the step after it.
    this.fishCount = Math.min(MAX_FISH, rng.int(5, 9));
    const middle = Math.max(1, Math.floor(pools / 2));
    for (let i = 0; i < this.fishCount; i++) {
      const k = i < 2 ? Math.min(pools - 1, middle + i) : rng.int(0, pools - 1);
      this.fishPool[i] = k;
      this.fishX[i] = i < 2 ? this.spotX(k) : rng.range(this.poolStart[k] + 1, this.poolEnd[k] - 1);
      this.fishY[i] = i < 2 ? this.spotY(k) : rng.range(1, RIVER_WIDTH - 1);
      this.fishZ[i] = this.poolSurface[k];
      this.fishVX[i] = this.fishVY[i] = this.fishPitch[i] = 0;
      this.fishTail[i] = rng.range(0, Math.PI * 2);
      this.fishScale[i] = rng.range(0.85, 1.12);
      this.fishLeapS[i] = this.fishLeapT[i] = 0;
      this.fishWait[i] = 0;
      this.fishPatience[i] = rng.range(PATIENCE_S[0], PATIENCE_S[1]);
      this.fishWander[i] = rng.range(0, Math.PI * 2);
    }
    // Clamp the scattered ones into their pools' swimming room.
    for (let i = 2; i < this.fishCount; i++) {
      const k = this.fishPool[i];
      const lo = k > 0 ? this.poolStart[k] + STEP_CLEARANCE : this.poolStart[k] + 0.5;
      const hi = k < pools - 1 ? this.poolEnd[k] - WEIR_CLEARANCE : this.poolEnd[k] - 0.5;
      this.fishX[i] = Math.max(lo, Math.min(hi, this.fishX[i]));
    }
    this.startLeap(0, true);
    this.fishLeapT[0] = LEAP_S * 0.45;
    this.flyLeap(0, 0);
    for (let i = 0; i < RIPPLE_POOL; i++) this.rippleStrength[i] = 0;
  }
}
