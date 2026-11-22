import type { ScopeFrame } from "../utils/scopeFeed";

/** The backend's floor for every level readout (`METER_FLOOR_DB` in `scope.rs`): silence, not a level. */
export const LEVEL_FLOOR_DB = -70;
/** Short-term loudness samples the history strip keeps: two minutes at one per ~200 ms. */
export const HISTORY_LENGTH = 600;
/** Scope frames (~30 a second) per history sample. */
const FRAMES_PER_HISTORY_SAMPLE = 6;
/** Most L/R pairs one scope frame carries (the backend's `MAX_POINTS` in `scope.rs`). */
const MAX_TRACE_POINTS = 512;

/**
 * What the readouts show, accumulated from scope frames: the backend's live meters plus
 * the two things only a reader can hold, true-peak hold and the short-term loudness history.
 * Both start afresh whenever the backend's meters do (seek, new track, device change).
 */
export class ScopeMeters {
  correlation = 0;
  momentaryLufs = LEVEL_FLOOR_DB;
  shortTermLufs = LEVEL_FLOOR_DB;
  /** Highest true peak since the meters last restarted, in dBTP. */
  peakHoldDb = LEVEL_FLOOR_DB;
  balanceDb = 0;
  /** Short-term loudness history, a ring: `historyCount` samples ending at `historyNewest`. */
  readonly history = new Float32Array(HISTORY_LENGTH).fill(LEVEL_FLOOR_DB);
  historyCount = 0;
  private historyNewest = HISTORY_LENGTH - 1;
  private framesSinceSample = FRAMES_PER_HISTORY_SAMPLE - 1;

  push(frame: ScopeFrame): void {
    if (frame.restarted) this.reset();
    this.correlation = frame.correlation;
    this.momentaryLufs = frame.momentaryLufs;
    this.shortTermLufs = frame.shortTermLufs;
    this.balanceDb = frame.balanceDb;
    this.peakHoldDb = Math.max(this.peakHoldDb, frame.truePeakDb);
    this.framesSinceSample++;
    if (this.framesSinceSample >= FRAMES_PER_HISTORY_SAMPLE) {
      this.framesSinceSample = 0;
      this.historyNewest = (this.historyNewest + 1) % HISTORY_LENGTH;
      this.history[this.historyNewest] = frame.shortTermLufs;
      this.historyCount = Math.min(HISTORY_LENGTH, this.historyCount + 1);
    }
  }

  reset(): void {
    this.peakHoldDb = LEVEL_FLOOR_DB;
    this.history.fill(LEVEL_FLOOR_DB);
    this.historyCount = 0;
    this.historyNewest = HISTORY_LENGTH - 1;
    this.framesSinceSample = FRAMES_PER_HISTORY_SAMPLE - 1;
  }

  /** The `age`-th most recent history sample (0 is the newest); only valid below `historyCount`. */
  historyAgo(age: number): number {
    return this.history[(this.historyNewest - age + HISTORY_LENGTH) % HISTORY_LENGTH];
  }
}

/**
 * The trace in goniometer space: L/R pairs rotated 45° onto mid (up) and side (right), so
 * mono is a vertical line, wide stereo a broad cloud and out-of-phase content leans
 * horizontal. A gentle automatic gain (at most 4×, 12 dB) keeps quiet passages readable;
 * full-scale mono reaches the top of the unit circle at gain 1.
 */
export class ScopeTrace {
  /** Interleaved mid/side points `[x0, y0, x1, y1, …]` in about [-1, 1]; `count` are live. */
  readonly points = new Float32Array(MAX_TRACE_POINTS * 2);
  count = 0;
  /** Linear gain applied to `points`. */
  gain = 1;
  private envelope = 0;

  /** Replace the trace with `pairs` (interleaved L/R, as the backend sends them). */
  set(pairs: readonly number[]): void {
    const n = Math.min(MAX_TRACE_POINTS, pairs.length >> 1);
    let peak = 0;
    for (let i = 0; i < n; i++) {
      const l = pairs[2 * i];
      const r = pairs[2 * i + 1];
      const x = (r - l) * Math.SQRT1_2;
      const y = (l + r) * Math.SQRT1_2;
      this.points[2 * i] = x;
      this.points[2 * i + 1] = y;
      peak = Math.max(peak, Math.abs(x), Math.abs(y));
    }
    this.count = n;
    // Fast attack, slow release, so a sudden loud passage never overshoots the frame by
    // much and the gain doesn't pump between frames.
    this.envelope = Math.max(peak, this.envelope * 0.97);
    this.gain = this.envelope > 0 ? Math.min(4, Math.max(1, 0.9 / this.envelope)) : 1;
    for (let i = 0; i < 2 * n; i++) this.points[i] *= this.gain;
  }

  clear(): void {
    this.count = 0;
  }
}

/** Labels the readouts need, in the current language. */
export interface ScopeLabels {
  readonly left: string;
  readonly right: string;
  readonly centre: string;
}

const MINUS = "−";

/** A level in LUFS or dB to one decimal with a true minus sign; the floor reads as "–". */
export function formatLevel(db: number): string {
  if (db <= LEVEL_FLOOR_DB) return "–";
  const text = Math.abs(db).toFixed(1);
  return db < 0 && text !== "0.0" ? MINUS + text : text;
}

/** Balance as the louder side and by how much, e.g. "L 1.2 dB"; under 0.1 dB reads as centred. */
export function formatBalance(db: number, labels: ScopeLabels): string {
  const text = Math.abs(db).toFixed(1);
  if (text === "0.0") return labels.centre;
  return `${db < 0 ? labels.left : labels.right} ${text} dB`;
}

/** Correlation to two decimals with a sign, e.g. "+0.87". */
export function formatCorrelation(value: number): string {
  const text = Math.abs(value).toFixed(2);
  if (text === "0.00") return text;
  return (value < 0 ? MINUS : "+") + text;
}
