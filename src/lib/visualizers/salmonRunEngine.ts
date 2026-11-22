import type { VisualizerFrame } from "./types";

// The river is simulated in world units: the canvas height is 1 and its width is the aspect
// ratio, so shapes keep their proportions on any stage. The current flows left to right; the
// salmon swim right to left, upstream.

/** Most salmon any river holds; the per-track count is drawn from `SalmonRunConfig.fish`. */
const MAX_FISH = 9;
/** Most boulders any river holds. */
const MAX_BOULDERS = 7;
/** Ripples in flight at once; a new one replaces the oldest beyond this. */
export const RIPPLE_POOL = 12;

/** Speed of the surface current at full activity, in world units per second. */
const FLOW_SPEED = 0.16;
/** A ripple's lifetime and how fast its ring widens. */
export const RIPPLE_LIFE_S = 1.6;
const RIPPLE_GROWTH = 0.11;
/** How long a leap lasts, from leaving the water to the landing splash. */
export const LEAP_S = 0.8;
/** Under reduced motion the leaf drifts at this fraction of its usual speed. */
const LEAF_REDUCED_PACE = 0.55;

/** Time constant of the river easing between calm (paused) and running, in seconds. */
const ACTIVITY_TAU_S = 0.5;
/** Upstream ground speed of a salmon: a slow base plus a share driven by mid/treble energy. */
const SWIM_BASE = 0.045;
const SWIM_ENERGY = 0.07;
/** Extra upstream speed at the start of a surge, decaying at `SURGE_DECAY` per second. */
const SURGE_SPEED = 0.32;
const SURGE_DECAY = 2.6;
/** Tail beats per second: a base plus a share driven by mid/treble energy. */
const TAIL_BASE_HZ = 1.4;
const TAIL_ENERGY_HZ = 2.6;
/** Under reduced motion salmon swim and beat their tails at this fraction of their usual pace. */
const REDUCED_PACE = 0.6;
/**
 * How far a station-holding salmon's home wanders from mid-frame, as fractions of the width and
 * height: enough to read as alive, little enough to stay clear of the edges.
 */
const HOLD_ROAM_X = 0.22;
const HOLD_ROAM_Y = 0.08;
/** Bank margin the salmon keep, as a fraction of the height. */
const BANK = 0.14;
/** Treble jump over its running average that counts as a transient. */
const TRANSIENT_JUMP = 0.22;
/** Onset strength that counts as a transient on its own. */
const TRANSIENT_ONSET = 0.75;
/** Transient strength above which a surge becomes a leap. */
const LEAP_THRESHOLD = 0.5;
/** Minimum seconds between two transients, so a busy hi-hat doesn't set the whole school off. */
const TRANSIENT_COOLDOWN_S = 0.6;
/** Reduced-motion "light shift" a beat leaves on the water, and its decay per second. */
const GLOW_PER_ONSET = 0.6;
const GLOW_DECAY = 1.2;
const ATTACK_RATE = 20;
const DECAY_RATE = 4;
const EPSILON = 0.002;

/** What one size of river contains. */
export interface SalmonRunConfig {
  /** Inclusive range of salmon per track, at most `MAX_FISH`. */
  readonly fish: readonly [number, number];
  /** Inclusive range of boulders per track, at most `MAX_BOULDERS`; [0, 0] for none. */
  readonly boulders: readonly [number, number];
  /** Current streaks drifting on the surface. */
  readonly streaks: number;
  /** Small riverbed stones. */
  readonly pebbles: number;
  /** Salmon length, in world units (fractions of the height). */
  readonly fishLength: number;
  /** Whether a leaf drifts past on a track change. */
  readonly leaf: boolean;
  /** The leaf's length, stem to tip, in world units. */
  readonly leafSize: number;
  /** Seconds the leaf takes to cross from edge to edge (slower under reduced motion). */
  readonly leafCrossS: number;
  /** Whether beats drop ripples on the water. */
  readonly ripples: boolean;
  /** What a treble transient does to a salmon: a surge forward, or a surge and a leap. */
  readonly transients: "surge" | "leap";
  /**
   * Salmon hold station mid-frame against the current, wandering slowly about, instead of
   * swimming upstream through the frame and rejoining downstream.
   */
  readonly holdStation: boolean;
}

/** A small, fast, seedable PRNG (mulberry32), so a track's river is the same every time. */
export class SeededRandom {
  private state: number;

  constructor(seed: number) {
    this.state = seed >>> 0;
  }

  reseed(seed: number): void {
    this.state = seed >>> 0;
  }

  /** Uniform in [0, 1). */
  next(): number {
    this.state = (this.state + 0x6d2b79f5) >>> 0;
    let t = this.state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }

  range(min: number, max: number): number {
    return min + (max - min) * this.next();
  }

  /** An integer in [min, max], inclusive. */
  int(min: number, max: number): number {
    return min + Math.floor(this.next() * (max - min + 1));
  }
}

/** FNV-1a hash of `key`, the seed for that track's river. */
export function seedFromKey(key: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    h ^= key.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function bandAverage(bands: readonly number[], from: number, to: number): number {
  let sum = 0;
  for (let i = from; i < to; i++) sum += bands[i] || 0;
  return sum / (to - from);
}

function approach(current: number, target: number, attack: number, decay: number): number {
  const next = current + (target - current) * (target >= current ? attack : decay);
  return Math.abs(target - next) < EPSILON ? target : next;
}

/**
 * The salmon run's state: a per-track riverbed (boulders, pebbles), current streaks, a school of
 * salmon, ripples, a drifting leaf and the smoothed audio levels driving them. Every array is
 * allocated once in the constructor; `push` and `step` only write into them.
 *
 * `activity` eases between 1 while playing and 0 while paused; flow and swimming scale with it,
 * so pausing calms the river and `step` returns false once it's still. The renderer reads the
 * public fields directly.
 */
export class SalmonRun {
  readonly config: SalmonRunConfig;
  /** Width of the world in height units; set from the canvas by `setAspect`. */
  aspect = 16 / 9;

  /** Smoothed band levels in [0, 1]. */
  bass = 0;
  mid = 0;
  treble = 0;
  /** 0 when calm (paused), 1 when running. */
  activity = 0;
  /** Reduced-motion light shift on the water from beats, in [0, 1]. */
  glow = 0;

  fishCount = 0;
  readonly fishX = new Float32Array(MAX_FISH);
  readonly fishY = new Float32Array(MAX_FISH);
  /** Ground velocity, world units per second; negative x is upstream. */
  readonly fishVX = new Float32Array(MAX_FISH);
  readonly fishVY = new Float32Array(MAX_FISH);
  /** Tail-beat phase in radians. */
  readonly fishTail = new Float32Array(MAX_FISH);
  /** Size multiplier, so the school isn't uniform. */
  readonly fishScale = new Float32Array(MAX_FISH);
  /** Surge boost in [0, 1]. */
  readonly fishSurge = new Float32Array(MAX_FISH);
  /** Seconds left in a leap; 0 when swimming. */
  readonly fishLeap = new Float32Array(MAX_FISH);
  private readonly fishWander = new Float32Array(MAX_FISH);
  private readonly fishWanderRate = new Float32Array(MAX_FISH);

  boulderCount = 0;
  /** Boulder centres as fractions of the width (x) and height (y), and radii in world units. */
  readonly boulderFX = new Float32Array(MAX_BOULDERS);
  readonly boulderY = new Float32Array(MAX_BOULDERS);
  readonly boulderR = new Float32Array(MAX_BOULDERS);

  /** Riverbed pebbles, one cell each: x as a fraction of the width, y in world units. */
  readonly pebbleFX: Float32Array;
  readonly pebbleY: Float32Array;

  /** Streak positions in world units, lengths in world units. */
  readonly streakX: Float32Array;
  readonly streakY: Float32Array;
  readonly streakLen: Float32Array;

  readonly rippleX = new Float32Array(RIPPLE_POOL);
  readonly rippleY = new Float32Array(RIPPLE_POOL);
  /** Seconds since the ripple started. */
  readonly rippleAge = new Float32Array(RIPPLE_POOL);
  /** Starting brightness in (0, 1]; 0 marks a free slot. */
  readonly rippleStrength = new Float32Array(RIPPLE_POOL);

  /** The leaf's position (world units); `leafActive` is false when none is drifting. */
  leafActive = false;
  leafX = 0;
  leafY = 0;

  /** Bumped whenever the riverbed is laid out again, so a cached drawing of it can tell it's stale. */
  layoutVersion = 0;

  private key: string | null = null;
  private readonly rng = new SeededRandom(0);
  private playing = false;
  private bassTarget = 0;
  private midTarget = 0;
  private trebleTarget = 0;
  /** Running average of raw treble, which transients are measured against. */
  private trebleAverage = 0;
  private cooldown = 0;
  /** A transient seen by `push`, applied on the next `step` (0 = none). */
  private pendingTransient = 0;
  private pendingOnset = 0;

  constructor(config: SalmonRunConfig) {
    this.config = config;
    this.pebbleFX = new Float32Array(config.pebbles);
    this.pebbleY = new Float32Array(config.pebbles);
    this.streakX = new Float32Array(config.streaks);
    this.streakY = new Float32Array(config.streaks);
    this.streakLen = new Float32Array(config.streaks);
    this.layOut("");
  }

  /**
   * Shows the river for the track `key`. A new key lays out a new riverbed and school from a
   * seed of the key (the same key always gives the same river) and, after the first track, sets
   * a leaf drifting downstream. The same key again is a no-op.
   */
  setTrack(key: string): void {
    if (key === this.key) return;
    const first = this.key === null;
    this.key = key;
    this.layOut(key);
    if (!first && this.config.leaf) {
      this.leafActive = true;
      this.leafX = -this.config.leafSize;
      this.leafY = this.rng.range(0.25, 0.75);
    }
  }

  /** Matches the world to the canvas shape; moving things keep their place across the width. */
  setAspect(aspect: number): void {
    if (!(aspect > 0) || Math.abs(aspect - this.aspect) < 1e-6) return;
    const scale = aspect / this.aspect;
    for (let i = 0; i < this.fishCount; i++) this.fishX[i] *= scale;
    for (let i = 0; i < this.streakX.length; i++) this.streakX[i] *= scale;
    for (let i = 0; i < RIPPLE_POOL; i++) this.rippleX[i] *= scale;
    this.leafX *= scale;
    this.aspect = aspect;
  }

  push(frame: VisualizerFrame): void {
    const resumed = frame.playing && !this.playing;
    this.playing = frame.playing;
    const bands = frame.bands;
    if (!frame.playing || bands.length === 0) {
      this.bassTarget = this.midTarget = this.trebleTarget = 0;
      return;
    }
    const onset = Math.min(1, frame.onset || 0);
    const treble = Math.min(1, bandAverage(bands, 20, 32) * 1.6);
    this.bassTarget = Math.max(Math.min(1, bandAverage(bands, 0, 8) * 1.4), onset);
    this.midTarget = Math.min(1, bandAverage(bands, 8, 20) * 1.4);
    this.trebleTarget = treble;

    // The first data after starting or resuming sets the baseline, so it is never a transient.
    if (resumed) this.trebleAverage = treble;
    const jump = treble - this.trebleAverage;
    this.trebleAverage += (treble - this.trebleAverage) * 0.1;
    const transient = Math.max(
      jump > TRANSIENT_JUMP ? Math.min(1, jump * 2.5) : 0,
      onset >= TRANSIENT_ONSET ? onset : 0,
    );
    if (transient > this.pendingTransient) this.pendingTransient = transient;
    if (onset > this.pendingOnset) this.pendingOnset = onset;
  }

  /**
   * Advances the river by `dtMs`. Returns true while anything is still moving: always while
   * playing, and while paused until the river has calmed and every ripple, leap and the leaf
   * are done.
   */
  step(dtMs: number, reducedMotion: boolean): boolean {
    const dt = Math.max(0, Math.min(100, dtMs)) / 1000;
    const attack = 1 - Math.exp(-ATTACK_RATE * dt);
    const decay = 1 - Math.exp(-DECAY_RATE * dt);
    this.bass = approach(this.bass, this.bassTarget, attack, decay);
    this.mid = approach(this.mid, this.midTarget, attack, decay);
    this.treble = approach(this.treble, this.trebleTarget, attack, decay);
    const activityTarget = this.playing ? 1 : 0;
    this.activity = approach(
      this.activity,
      activityTarget,
      1 - Math.exp(-dt / ACTIVITY_TAU_S),
      1 - Math.exp(-dt / ACTIVITY_TAU_S),
    );
    if (Math.abs(this.activity - activityTarget) < 0.01) this.activity = activityTarget;

    this.cooldown = Math.max(0, this.cooldown - dt);
    this.applyBeats(reducedMotion);
    this.glow = this.glow > EPSILON ? Math.max(0, this.glow - GLOW_DECAY * dt) : 0;

    const flow = FLOW_SPEED * this.activity;
    this.moveStreaks(flow * dt);
    this.moveSchool(dt, reducedMotion);
    const rippling = this.moveRipples(dt, flow, reducedMotion);
    this.moveLeaf(dt, reducedMotion);

    let leaping = false;
    for (let i = 0; i < this.fishCount; i++) {
      if (this.fishLeap[i] > 0 || this.fishSurge[i] > 0) leaping = true;
    }
    return (
      this.playing ||
      this.activity > 0 ||
      rippling ||
      leaping ||
      this.leafActive ||
      this.glow > 0 ||
      this.bass !== this.bassTarget ||
      this.mid !== this.midTarget ||
      this.treble !== this.trebleTarget
    );
  }

  /** Starts a ripple at (`x`, `y`) in a free slot, or over the oldest one. */
  private spawnRipple(x: number, y: number, strength: number): void {
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
    this.rippleAge[slot] = 0;
    this.rippleStrength[slot] = Math.max(0.05, Math.min(1, strength));
  }

  /** Turns the beats `push` saw into ripples, surges and leaps (or, reduced, a light shift). */
  private applyBeats(reducedMotion: boolean): void {
    const onset = this.pendingOnset;
    const transient = this.pendingTransient;
    this.pendingOnset = this.pendingTransient = 0;
    if (reducedMotion) {
      if (onset > 0) this.glow = Math.max(this.glow, onset * GLOW_PER_ONSET);
      return;
    }
    if (onset > 0 && this.config.ripples) {
      // A riffle: just downstream of a boulder, or anywhere on the water if there are none.
      if (this.boulderCount > 0) {
        const b = this.rng.int(0, this.boulderCount - 1);
        const r = this.boulderR[b];
        this.spawnRipple(
          this.boulderFX[b] * this.aspect + r * 1.4,
          this.boulderY[b] + this.rng.range(-r, r) * 0.6,
          0.4 + 0.6 * onset,
        );
      } else {
        this.spawnRipple(
          this.rng.range(0.1, 0.9) * this.aspect,
          this.rng.range(0.2, 0.8),
          0.4 + 0.6 * onset,
        );
      }
    }
    if (transient > 0 && this.cooldown === 0 && this.fishCount > 0) {
      this.cooldown = TRANSIENT_COOLDOWN_S;
      // The first fish not already surging or leaping, starting from a random one.
      const start = this.rng.int(0, this.fishCount - 1);
      for (let k = 0; k < this.fishCount; k++) {
        const i = (start + k) % this.fishCount;
        if (this.fishLeap[i] > 0 || this.fishSurge[i] > 0.2) continue;
        this.fishSurge[i] = 1;
        if (this.config.transients === "leap" && transient >= LEAP_THRESHOLD) {
          this.fishLeap[i] = LEAP_S;
          if (this.config.ripples) this.spawnRipple(this.fishX[i], this.fishY[i], 0.9);
        }
        break;
      }
    }
  }

  private moveStreaks(dx: number): void {
    const n = this.streakX.length;
    for (let i = 0; i < n; i++) {
      // Faster mid-channel than near the banks.
      const lane = 1 - Math.abs(this.streakY[i] - 0.5) * 1.2;
      this.streakX[i] += dx * lane;
      if (this.streakX[i] - this.streakLen[i] > this.aspect) this.streakX[i] -= this.aspect + 0.3;
    }
  }

  private moveSchool(dt: number, reducedMotion: boolean): void {
    const n = this.fishCount;
    const a = this.activity;
    const pace = reducedMotion ? REDUCED_PACE : 1;
    const energy = (this.mid + this.treble) * 0.5;
    const len = this.config.fishLength;
    const sepRange = len * 2.2;

    let meanY = 0;
    for (let i = 0; i < n; i++) meanY += this.fishY[i];
    meanY = n > 0 ? meanY / n : 0.5;

    for (let i = 0; i < n; i++) {
      const surge = this.fishSurge[i];
      const speed = (SWIM_BASE + SWIM_ENERGY * energy + SURGE_SPEED * surge) * a * pace;
      const x = this.fishX[i];
      const y = this.fishY[i];

      // Steering, as a vertical drift plus a nudge to the upstream speed.
      this.fishWander[i] += this.fishWanderRate[i] * dt * a;
      let steerY = Math.sin(this.fishWander[i]) * 0.035 + (meanY - y) * 0.08;
      let steerX = 0;
      for (let j = 0; j < n; j++) {
        if (j === i) continue;
        const dx = x - this.fishX[j];
        const dy = y - this.fishY[j];
        const d = Math.hypot(dx, dy);
        if (d > 0 && d < sepRange) {
          const push = ((sepRange - d) / sepRange) * 0.25;
          steerX += (dx / d) * push;
          steerY += (dy / d) * push;
        }
      }
      for (let b = 0; b < this.boulderCount; b++) {
        const dx = x - this.boulderFX[b] * this.aspect;
        const dy = y - this.boulderY[b];
        const d = Math.hypot(dx, dy);
        const reach = this.boulderR[b] + len * 1.2;
        if (d > 0 && d < reach) {
          const push = ((reach - d) / reach) * 0.6;
          steerX += (dx / d) * push * 0.4;
          steerY += (dy / d) * push;
        }
      }
      if (y < BANK) steerY += (BANK - y) * 1.5;
      if (y > 1 - BANK) steerY -= (y - (1 - BANK)) * 1.5;

      let targetVX: number;
      let targetVY: number;
      if (this.config.holdStation) {
        // Matching the current around a home that wanders slowly about mid-frame; a surge darts
        // it forward and the current carries it back.
        const w = this.fishWander[i];
        const homeX = this.aspect * (0.5 + HOLD_ROAM_X * Math.sin(w * 0.6));
        const homeY = 0.5 + HOLD_ROAM_Y * Math.sin(w * 1.3);
        targetVX = ((homeX - x) * 0.8 - SURGE_SPEED * surge * pace) * a;
        targetVY = (homeY - y) * 1.2 * a * pace;
      } else {
        // Steering may slow a salmon but never turn it downstream: it goes around, not back.
        targetVX = Math.min(-speed + steerX * a * pace, -speed * 0.3);
        targetVY = steerY * a * pace;
      }
      const ease = 1 - Math.exp(-3 * dt);
      this.fishVX[i] += (targetVX - this.fishVX[i]) * ease;
      this.fishVY[i] += (targetVY - this.fishVY[i]) * ease;
      this.fishX[i] = x + this.fishVX[i] * dt;
      this.fishY[i] = Math.max(0.04, Math.min(0.96, y + this.fishVY[i] * dt));

      const hz = (TAIL_BASE_HZ + TAIL_ENERGY_HZ * energy + 2 * surge) * a * pace;
      this.fishTail[i] = (this.fishTail[i] + hz * dt * Math.PI * 2) % (Math.PI * 2);

      this.fishSurge[i] = surge > EPSILON ? surge * Math.exp(-SURGE_DECAY * dt) : 0;
      if (this.fishLeap[i] > 0) {
        this.fishLeap[i] = Math.max(0, this.fishLeap[i] - dt);
        if (this.fishLeap[i] === 0 && this.config.ripples) {
          this.spawnRipple(this.fishX[i], this.fishY[i], 1);
        }
      }

      // Past the upstream edge: rejoin the run downstream, out of sight.
      if (!this.config.holdStation && this.fishX[i] < -len * 1.5) {
        this.fishX[i] = this.aspect + len * this.rng.range(1.5, 4);
        this.fishY[i] = this.rng.range(BANK + 0.05, 1 - BANK - 0.05);
        this.fishLeap[i] = 0;
      }
    }
  }

  private moveRipples(dt: number, flow: number, reducedMotion: boolean): boolean {
    let any = false;
    for (let i = 0; i < RIPPLE_POOL; i++) {
      if (this.rippleStrength[i] === 0) continue;
      this.rippleAge[i] += dt;
      this.rippleX[i] += flow * dt * 0.8;
      if (reducedMotion || this.rippleAge[i] >= RIPPLE_LIFE_S) this.rippleStrength[i] = 0;
      else any = true;
    }
    return any;
  }

  private moveLeaf(dt: number, reducedMotion: boolean): void {
    if (!this.leafActive) return;
    const size = this.config.leafSize;
    const speed = (this.aspect + 2 * size) / this.config.leafCrossS;
    this.leafX += speed * dt * (reducedMotion ? LEAF_REDUCED_PACE : 1);
    this.leafY += Math.sin((this.leafX / size) * 0.3) * size * 0.15 * dt;
    if (this.leafX > this.aspect + size) this.leafActive = false;
  }

  /** Radius of ripple `i`, in world units. */
  rippleRadius(i: number): number {
    return 0.01 + RIPPLE_GROWTH * this.rippleAge[i];
  }

  /** Brightness in [0, 1] of ripple `i`, fading over its life. */
  rippleAlpha(i: number): number {
    const s = this.rippleStrength[i];
    return s === 0 ? 0 : s * (1 - this.rippleAge[i] / RIPPLE_LIFE_S);
  }

  /** Lays out the riverbed, streaks and school from the seed of `key`. */
  private layOut(key: string): void {
    const rng = this.rng;
    rng.reseed(seedFromKey(key));
    this.layoutVersion++;
    const c = this.config;

    this.boulderCount = Math.min(MAX_BOULDERS, rng.int(c.boulders[0], c.boulders[1]));
    let placed = 0;
    // Boulders are spread across the width and kept apart; a crowded draw just yields fewer.
    for (let tries = 0; placed < this.boulderCount && tries < 60; tries++) {
      const fx = rng.range(0.08, 0.92);
      const y = rng.range(0.2, 0.8);
      const r = rng.range(0.04, 0.085);
      let clear = true;
      for (let k = 0; k < placed; k++) {
        const dx = (fx - this.boulderFX[k]) * this.aspect;
        const dy = y - this.boulderY[k];
        if (Math.hypot(dx, dy) < r + this.boulderR[k] + 0.12) clear = false;
      }
      if (!clear) continue;
      this.boulderFX[placed] = fx;
      this.boulderY[placed] = y;
      this.boulderR[placed] = r;
      placed++;
    }
    this.boulderCount = placed;

    for (let i = 0; i < c.pebbles; i++) {
      this.pebbleFX[i] = rng.next();
      this.pebbleY[i] = rng.range(0.03, 0.97);
    }
    for (let i = 0; i < c.streaks; i++) {
      this.streakX[i] = rng.range(-0.2, this.aspect + 0.1);
      this.streakY[i] = rng.range(0.06, 0.94);
      this.streakLen[i] = rng.range(0.06, 0.18);
    }

    this.fishCount = Math.min(MAX_FISH, rng.int(c.fish[0], c.fish[1]));
    for (let i = 0; i < this.fishCount; i++) {
      if (c.holdStation) {
        this.fishX[i] = this.aspect * 0.5;
        this.fishY[i] = 0.5;
      } else {
        this.fishX[i] = rng.range(0.1, 0.9) * this.aspect;
        this.fishY[i] = rng.range(BANK + 0.05, 1 - BANK - 0.05);
      }
      this.fishVX[i] = -SWIM_BASE;
      this.fishVY[i] = 0;
      this.fishTail[i] = rng.range(0, Math.PI * 2);
      this.fishScale[i] = rng.range(0.82, 1.15);
      this.fishSurge[i] = 0;
      this.fishLeap[i] = 0;
      this.fishWander[i] = rng.range(0, Math.PI * 2);
      this.fishWanderRate[i] = rng.range(0.35, 0.8);
    }
  }
}
