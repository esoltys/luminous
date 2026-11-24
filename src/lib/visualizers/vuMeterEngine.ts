/**
 * Level maths for the VU meter visualizer: per-channel RMS from the scope feed's L/R
 * pairs, VU needle ballistics, and the two peak lamps. Pure state and arithmetic, no
 * canvas, so it is unit-tested directly.
 */

/**
 * Loudness (BS.1770) adds the two channels' power, so a stereo signal with both channels
 * at the same RMS reads about 3 dB higher in LUFS than either channel does in dBFS.
 */
const CHANNEL_SUM_DB = 3.01;
/**
 * A needle follows the short-term level, which in music swings well above the integrated
 * loudness a normalization target describes (its 300 ms peaks run about this far over).
 * Decks are calibrated the same way: program material averages around -6 VU and only its
 * peaks reach 0, so the red zone means "louder than the target" rather than "typical".
 */
const PROGRAM_HEADROOM_DB = 6;

/**
 * The per-channel RMS level, in dBFS, of the 0 VU mark, where the red zone begins, for
 * music normalized to `targetLufs`. An approximation: K-weighting, which emphasises highs,
 * moves real music by another dB or two either way.
 */
export function referenceForTargetLufs(targetLufs: number): number {
  return targetLufs - CHANNEL_SUM_DB + PROGRAM_HEADROOM_DB;
}

/** The 0 VU level for the backend's default normalization target (-16 LUFS). */
export const VU_REFERENCE_DBFS = referenceForTargetLufs(-16);

/** The needle can swing a little past the end of the scale before it hits the pin. */
export const NEEDLE_MAX = 1.03;

/**
 * The K-550's printed scale: where each VU mark sits along the sweep (0 = left stop,
 * 1 = end of scale), measured from the deck. Marks bunch toward the left and spread out
 * toward 0, so 0 VU is about 70% of the way across. Between marks the needle position is
 * interpolated in dB.
 */
const SCALE: readonly (readonly [vu: number, position: number])[] = [
  [-20, 0],
  [-10, 0.195],
  [-7, 0.31],
  [-5, 0.43],
  [-3, 0.534],
  [0, 0.696],
  [3, 0.852],
  [5, 1],
];
/** Position gained per dB beyond the last mark, until the pin. */
const OVERSCALE_PER_DB = 0.074;

/**
 * Needle dynamics as a damped spring. The VU standard asks for 99% of a step in 300 ms
 * with about 1.5% overshoot: a damping ratio of 0.8 gives that overshoot, and this natural
 * frequency (rad/s) settles in about the right time.
 */
const NEEDLE_DAMPING = 0.8;
const NEEDLE_OMEGA = 19;
/** The integrator's longest step in ms; a long frame gap is taken in several steps. */
const MAX_SUBSTEP_MS = 8;
/** Below this distance and speed a needle counts as at rest. */
const REST_EPSILON = 0.0005;

/**
 * Peak lamp thresholds in dBTP, the frame's true peak. Real decks calibrate these against
 * 0 VU, but modern masters peak near full scale all the time, so the lamps are set to mean
 * something on a digital source: the +3 lamp marks a very hot peak and the +8 lamp an
 * over (a true peak at or above full scale).
 */
const PEAK_LAMP_LOW_DBTP = -4;
const PEAK_LAMP_HIGH_DBTP = 0;
/** A lit lamp stays at full brightness this long, then fades with the given time constant. */
const LAMP_HOLD_MS = 250;
const LAMP_FADE_TAU_MS = 140;
/** Below this brightness a fading lamp counts as off. */
const LAMP_OFF = 0.02;

/** The needle position (0..NEEDLE_MAX) for a level of `vu` dB relative to 0 VU. */
export function deflectionForVu(vu: number): number {
  if (vu <= SCALE[0][0]) return 0;
  for (let i = 1; i < SCALE.length; i++) {
    const [vu1, f1] = SCALE[i];
    if (vu <= vu1) {
      const [vu0, f0] = SCALE[i - 1];
      return f0 + ((f1 - f0) * (vu - vu0)) / (vu1 - vu0);
    }
  }
  return Math.min(NEEDLE_MAX, 1 + (vu - SCALE[SCALE.length - 1][0]) * OVERSCALE_PER_DB);
}

/** The needle position for an RMS amplitude (full scale = 1), with 0 VU at `referenceDbfs`. */
export function deflectionForRms(rms: number, referenceDbfs = VU_REFERENCE_DBFS): number {
  if (!(rms > 0)) return 0;
  return deflectionForVu(20 * Math.log10(rms) - referenceDbfs);
}

/** RMS and highest sample of each channel in interleaved `[l0, r0, l1, r1, …]` pairs. */
export function channelLevels(points: readonly number[]): {
  rmsL: number;
  rmsR: number;
  peak: number;
} {
  const pairs = points.length >> 1;
  if (pairs === 0) return { rmsL: 0, rmsR: 0, peak: 0 };
  let sumL = 0;
  let sumR = 0;
  let peak = 0;
  for (let i = 0; i < pairs; i++) {
    const l = points[2 * i];
    const r = points[2 * i + 1];
    sumL += l * l;
    sumR += r * r;
    peak = Math.max(peak, Math.abs(l), Math.abs(r));
  }
  return { rmsL: Math.sqrt(sumL / pairs), rmsR: Math.sqrt(sumR / pairs), peak };
}

/** One needle: a damped spring chasing a target deflection. */
export class Needle {
  position = 0;
  private velocity = 0;
  private target = 0;

  setTarget(target: number): void {
    this.target = Math.max(0, Math.min(NEEDLE_MAX, target));
  }

  /** Advance by `dtMs`; with `instant` the needle goes straight to its target. */
  step(dtMs: number, instant: boolean): void {
    if (instant) {
      this.position = this.target;
      this.velocity = 0;
      return;
    }
    let remaining = dtMs;
    while (remaining > 0) {
      const dt = Math.min(remaining, MAX_SUBSTEP_MS) / 1000;
      remaining -= MAX_SUBSTEP_MS;
      const accel =
        NEEDLE_OMEGA * NEEDLE_OMEGA * (this.target - this.position) -
        2 * NEEDLE_DAMPING * NEEDLE_OMEGA * this.velocity;
      this.velocity += accel * dt;
      this.position += this.velocity * dt;
    }
    // The pin stops the needle dead at either end.
    if (this.position < 0) {
      this.position = 0;
      this.velocity = Math.max(0, this.velocity);
    } else if (this.position > NEEDLE_MAX) {
      this.position = NEEDLE_MAX;
      this.velocity = Math.min(0, this.velocity);
    }
  }

  /** Signed velocity in needle units per second, for effects that follow motion. */
  get speed(): number {
    return this.velocity;
  }

  get atRest(): boolean {
    return (
      Math.abs(this.position - this.target) < REST_EPSILON && Math.abs(this.velocity) < REST_EPSILON
    );
  }
}

/** One peak lamp: lights when a threshold is crossed, holds, then fades. */
class Lamp {
  /** Brightness in [0, 1]. */
  level = 0;
  private sinceLit = Number.MAX_SAFE_INTEGER;

  light(): void {
    this.level = 1;
    this.sinceLit = 0;
  }

  step(dtMs: number, reducedMotion: boolean): void {
    if (this.level === 0) return;
    this.sinceLit += dtMs;
    if (this.sinceLit <= LAMP_HOLD_MS) return;
    if (reducedMotion) {
      this.level = 0;
      return;
    }
    this.level *= Math.exp(-dtMs / LAMP_FADE_TAU_MS);
    if (this.level < LAMP_OFF) this.level = 0;
  }
}

/** Both needles and both peak lamps, fed with scope frames. */
export class VuMeters {
  readonly left = new Needle();
  readonly right = new Needle();
  /** The +3 dB lamp. */
  readonly lampLow = new Lamp();
  /** The +8 dB lamp. */
  readonly lampHigh = new Lamp();

  /**
   * Take a scope frame's L/R pairs as the new level, with 0 VU at `referenceDbfs`. The
   * lamps read `peakDbtp`, the frame's true peak, which sees inter-sample peaks the
   * thinned pairs miss; without it they fall back to the highest sample in `points`.
   */
  push(points: readonly number[], referenceDbfs = VU_REFERENCE_DBFS, peakDbtp?: number): void {
    const { rmsL, rmsR, peak } = channelLevels(points);
    this.left.setTarget(deflectionForRms(rmsL, referenceDbfs));
    this.right.setTarget(deflectionForRms(rmsR, referenceDbfs));
    const peakDb = peakDbtp ?? (peak > 0 ? 20 * Math.log10(peak) : -Infinity);
    if (peakDb >= PEAK_LAMP_LOW_DBTP) this.lampLow.light();
    if (peakDb >= PEAK_LAMP_HIGH_DBTP) this.lampHigh.light();
  }

  /** Playback stopped or paused: the needles fall back to rest. */
  rest(): void {
    this.left.setTarget(0);
    this.right.setTarget(0);
  }

  /** Advance the needles and lamps; true while anything is still moving or lit. */
  step(dtMs: number, reducedMotion: boolean): boolean {
    this.left.step(dtMs, reducedMotion);
    this.right.step(dtMs, reducedMotion);
    this.lampLow.step(dtMs, reducedMotion);
    this.lampHigh.step(dtMs, reducedMotion);
    return (
      !this.left.atRest || !this.right.atRest || this.lampLow.level > 0 || this.lampHigh.level > 0
    );
  }
}
