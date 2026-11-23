import type { VisualizerFrame } from "./types";

// What the salmon run's two scenes share: a seedable PRNG so a track's layout is the same every
// time, and `RiverAudio`, which turns audio frames into the smoothed levels, beats and treble
// transients the scenes react to. `LoneSalmon` is the playbar's scene; the immersive fish ladder
// is in `fishLadderEngine.ts`.

const ATTACK_RATE = 20;
const DECAY_RATE = 4;
const EPSILON = 0.002;
/** Time constant of easing between calm (paused) and running, in seconds. */
const ACTIVITY_TAU_S = 0.5;
/** Treble jump over its running average that counts as a transient. */
const TRANSIENT_JUMP = 0.22;
/** Onset strength that counts as a transient on its own. */
const TRANSIENT_ONSET = 0.75;
/** Minimum seconds between two transients, so a busy hi-hat doesn't set everything off. */
const TRANSIENT_COOLDOWN_S = 0.6;

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
 * What a river hears: band levels smoothed toward each frame of data, an `activity` level that
 * eases between 0 (paused) and 1 (playing), and the beats (onsets) and sharp treble transients
 * seen since the last `step`, each handed out once. Transients are rate-limited, so a busy hi-hat
 * gives one every `TRANSIENT_COOLDOWN_S` at most.
 */
export class RiverAudio {
  /** Smoothed band levels in [0, 1]. */
  bass = 0;
  mid = 0;
  treble = 0;
  /** 0 when calm (paused), 1 when running. */
  activity = 0;
  playing = false;
  /** The strongest onset and transient since the last `step`, in (0, 1]; 0 for none. */
  onset = 0;
  transient = 0;

  private bassTarget = 0;
  private midTarget = 0;
  private trebleTarget = 0;
  /** Running average of raw treble, which transients are measured against. */
  private trebleAverage = 0;
  private cooldown = 0;
  private pendingOnset = 0;
  private pendingTransient = 0;

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
   * Advances the smoothing by `dt` seconds and moves the beats `push` saw into `onset` and
   * `transient`, for this step only.
   */
  step(dt: number): void {
    const attack = 1 - Math.exp(-ATTACK_RATE * dt);
    const decay = 1 - Math.exp(-DECAY_RATE * dt);
    this.bass = approach(this.bass, this.bassTarget, attack, decay);
    this.mid = approach(this.mid, this.midTarget, attack, decay);
    this.treble = approach(this.treble, this.trebleTarget, attack, decay);
    const activityTarget = this.playing ? 1 : 0;
    const ease = 1 - Math.exp(-dt / ACTIVITY_TAU_S);
    this.activity = approach(this.activity, activityTarget, ease, ease);
    if (Math.abs(this.activity - activityTarget) < 0.01) this.activity = activityTarget;

    this.cooldown = Math.max(0, this.cooldown - dt);
    this.onset = this.pendingOnset;
    this.transient = this.cooldown === 0 ? this.pendingTransient : 0;
    if (this.transient > 0) this.cooldown = TRANSIENT_COOLDOWN_S;
    this.pendingOnset = this.pendingTransient = 0;
  }

  /** Mid and treble together, in [0, 1]: what sets the pace of swimming. */
  get energy(): number {
    return (this.mid + this.treble) * 0.5;
  }

  /** Whether the music still moves the river: playing, or easing toward calm after a pause. */
  get stirring(): boolean {
    return (
      this.playing ||
      this.activity > 0 ||
      this.bass !== this.bassTarget ||
      this.mid !== this.midTarget ||
      this.treble !== this.trebleTarget
    );
  }
}

// The lone salmon is simulated in world units: the canvas height is 1 and its width is the aspect
// ratio, so shapes keep their proportions on any slot. The unseen current flows left to right.

/** Extra upstream speed at the start of a surge, decaying at `SURGE_DECAY` per second. */
const SURGE_SPEED = 0.32;
const SURGE_DECAY = 2.6;
/** Tail beats per second: a base plus a share driven by mid/treble energy. */
const TAIL_BASE_HZ = 1.4;
const TAIL_ENERGY_HZ = 2.6;
/** Under reduced motion the salmon beats its tail, and the leaf drifts, this much slower. */
const REDUCED_PACE = 0.6;
const LEAF_REDUCED_PACE = 0.55;
/** How far the salmon's home wanders from mid-frame, as fractions of the width and height. */
const HOLD_ROAM_X = 0.22;
const HOLD_ROAM_Y = 0.08;

/** The playbar's scene. */
export interface LoneSalmonConfig {
  /** Salmon length, in world units (fractions of the height). */
  readonly fishLength: number;
  /** The leaf's length, stem to tip, in world units. */
  readonly leafSize: number;
  /** Seconds the leaf takes to cross from edge to edge (slower under reduced motion). */
  readonly leafCrossS: number;
}

/**
 * The playbar's salmon run: one salmon holding station mid-frame against an unseen current,
 * wandering slowly about and darting forward on a treble transient, and a leaf drifting past on
 * a track change. Allocates nothing after construction; the renderer reads the fields directly.
 */
export class LoneSalmon {
  readonly config: LoneSalmonConfig;
  readonly audio = new RiverAudio();
  /** Width of the world in height units; set from the canvas by `setAspect`. */
  aspect = 16 / 9;

  fishX = 0;
  fishY = 0.5;
  /** Ground velocity, world units per second; negative x is upstream. */
  fishVX = 0;
  fishVY = 0;
  /** Tail-beat phase in radians. */
  fishTail = 0;
  /** Surge boost in [0, 1]. */
  fishSurge = 0;
  private wander = 0;

  /** The leaf's position (world units); `leafActive` is false when none is drifting. */
  leafActive = false;
  leafX = 0;
  leafY = 0;

  private key: string | null = null;
  private readonly rng = new SeededRandom(0);

  constructor(config: LoneSalmonConfig) {
    this.config = config;
    this.fishX = this.aspect * 0.5;
  }

  /**
   * Shows the salmon for the track `key`. After the first track, a new key sets a leaf drifting
   * past; the same key again is a no-op.
   */
  setTrack(key: string): void {
    if (key === this.key) return;
    const first = this.key === null;
    this.key = key;
    this.rng.reseed(seedFromKey(key));
    this.wander = this.rng.range(0, Math.PI * 2);
    if (!first) {
      this.leafActive = true;
      this.leafX = -this.config.leafSize;
      this.leafY = this.rng.range(0.25, 0.75);
    }
  }

  /** Matches the world to the canvas shape; the salmon and leaf keep their place across it. */
  setAspect(aspect: number): void {
    if (!(aspect > 0) || Math.abs(aspect - this.aspect) < 1e-6) return;
    const scale = aspect / this.aspect;
    this.fishX *= scale;
    this.leafX *= scale;
    this.aspect = aspect;
  }

  push(frame: VisualizerFrame): void {
    this.audio.push(frame);
  }

  /** Advances by `dtMs`; returns true while anything is still moving. */
  step(dtMs: number, reducedMotion: boolean): boolean {
    const dt = Math.max(0, Math.min(100, dtMs)) / 1000;
    const audio = this.audio;
    audio.step(dt);
    const a = audio.activity;
    const pace = reducedMotion ? REDUCED_PACE : 1;
    if (!reducedMotion && audio.transient > 0) this.fishSurge = 1;

    // Matching the current around a home that wanders slowly about mid-frame; a surge darts it
    // forward and the current carries it back.
    this.wander += 0.55 * dt * a;
    const homeX = this.aspect * (0.5 + HOLD_ROAM_X * Math.sin(this.wander * 0.6));
    const homeY = 0.5 + HOLD_ROAM_Y * Math.sin(this.wander * 1.3);
    const targetVX = ((homeX - this.fishX) * 0.8 - SURGE_SPEED * this.fishSurge * pace) * a;
    const targetVY = (homeY - this.fishY) * 1.2 * a * pace;
    const ease = 1 - Math.exp(-3 * dt);
    this.fishVX += (targetVX - this.fishVX) * ease;
    this.fishVY += (targetVY - this.fishVY) * ease;
    this.fishX += this.fishVX * dt;
    this.fishY = Math.max(0.04, Math.min(0.96, this.fishY + this.fishVY * dt));
    const hz = (TAIL_BASE_HZ + TAIL_ENERGY_HZ * audio.energy + 2 * this.fishSurge) * a * pace;
    this.fishTail = (this.fishTail + hz * dt * Math.PI * 2) % (Math.PI * 2);
    this.fishSurge = this.fishSurge > 0.002 ? this.fishSurge * Math.exp(-SURGE_DECAY * dt) : 0;

    if (this.leafActive) {
      const size = this.config.leafSize;
      const speed = (this.aspect + 2 * size) / this.config.leafCrossS;
      this.leafX += speed * dt * (reducedMotion ? LEAF_REDUCED_PACE : 1);
      this.leafY += Math.sin((this.leafX / size) * 0.3) * size * 0.15 * dt;
      if (this.leafX > this.aspect + size) this.leafActive = false;
    }
    return audio.stirring || this.fishSurge > 0 || this.leafActive;
  }
}
