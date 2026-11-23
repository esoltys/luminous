import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, isLightColor, mixHex } from "../utils/colorUtils";
import {
  FACE_ALL,
  FACE_LEFT,
  FACE_RIGHT,
  FACE_TOP,
  IsoView,
  isoFaces,
  type IsoFaces,
} from "./isoVoxel";
import { voxelTrackHighlight } from "./voxelGrid";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerRenderOptions,
  VisualizerSize,
  VisualizerTrack,
} from "./types";

const FULL_COLS = 16;
const FULL_DEPTH = 16;
const MAX_CUBES = 7;
const MINI_COLS = 10;
const MINI_DEPTH = 1;
const MINI_CUBES = 4;

const SNAPSHOT_MS = 85;
const ATTACK_RATE = 28;
const DECAY_RATE = 10;
const EPSILON = 0.004;

const PREVIEW_PROFILE = [0.3, 0.55, 0.8, 0.65, 0.9, 0.72, 0.5, 0.85, 0.7, 0.45] as const;

interface IsoSpectrumPalette {
  /** One `IsoFaces` per depth slice from back (0, oldest) to front (depth - 1, newest). */
  readonly slices: readonly IsoFaces[];
  /** Crest / top-cube highlight for the newest front slice. */
  readonly crest: IsoFaces;
  /** Subtle floor plane under the waterfall. */
  readonly floor: string;
}

function buildIsoPalette(
  colors: ThemeColors,
  depth: number,
  surface: "bg-main" | "bg-playerbar",
  trackAccent?: string | null,
): IsoSpectrumPalette {
  const bg = colors[surface] || colors["bg-main"] || "#191918";
  const light = isLightColor(bg);
  const accent = clampForContrast(colors["color-accent"] || "#4f5bd5", bg, 3.0);
  const baseHover = clampForContrast(colors["color-accent-hover"] || accent, bg, 3.0);
  const frontHex = voxelTrackHighlight(baseHover, bg, trackAccent, 0.55);
  const text = colors["color-text-primary"] || (light ? "#16181d" : "#f1f3f8");
  const crestHex = clampForContrast(mixHex(frontHex, text, 0.3), bg, 4.5);

  const slices: IsoFaces[] = new Array(depth);
  for (let d = 0; d < depth; d++) {
    const t = depth <= 1 ? 1 : d / (depth - 1);
    // Back slices recede toward the surface; front slice pops in full accent/artwork hue.
    const hex = mixHex(mixHex(bg, accent, light ? 0.25 : 0.3), frontHex, 0.2 + 0.8 * t);
    slices[d] = isoFaces(hex);
  }
  return {
    slices,
    crest: isoFaces(crestHex),
    floor: mixHex(bg, colors["color-border"] || accent, light ? 0.14 : 0.12),
  };
}

class VoxelIsoInstance implements VisualizerInstance {
  private readonly cols: number;
  private readonly depth: number;
  private readonly maxCubes: number;
  private readonly surface: "bg-main" | "bg-playerbar";
  private readonly targets: Float32Array;
  private readonly current: Float32Array;
  /** Ring buffer of `depth` slices, each `cols` floats; `head` is the newest slice. */
  private readonly history: Float32Array;
  private head = 0;
  private snapshotElapsedMs = 0;
  private playing = false;

  private readonly view = new IsoView();
  private fitWidth = 0;
  private fitHeight = 0;
  private colors!: ThemeColors;
  private trackAccent: string | null = null;
  private palette!: IsoSpectrumPalette;

  constructor(size: VisualizerSize) {
    const isMini = size === "mini";
    this.cols = isMini ? MINI_COLS : FULL_COLS;
    this.depth = isMini ? MINI_DEPTH : FULL_DEPTH;
    this.maxCubes = isMini ? MINI_CUBES : MAX_CUBES;
    this.surface = isMini ? "bg-playerbar" : "bg-main";
    this.targets = new Float32Array(this.cols);
    this.current = new Float32Array(this.cols);
    this.history = new Float32Array(this.cols * this.depth);
  }

  setTheme(colors: ThemeColors): void {
    this.colors = colors;
    this.palette = buildIsoPalette(colors, this.depth, this.surface, this.trackAccent);
  }

  setTrack(track: VisualizerTrack | null): void {
    const nextAccent = track?.accent ?? null;
    if (nextAccent !== this.trackAccent) {
      this.trackAccent = nextAccent;
      if (this.colors) {
        this.palette = buildIsoPalette(this.colors, this.depth, this.surface, nextAccent);
      }
    }
  }

  push(frame: VisualizerFrame): void {
    this.playing = frame.playing;
    if (!frame.playing) {
      this.targets.fill(0);
      return;
    }
    const bands = frame.bands;
    const bandCount = bands.length;
    if (bandCount === 0) {
      this.targets.fill(0);
      return;
    }
    for (let c = 0; c < this.cols; c++) {
      const start = Math.floor((c * bandCount) / this.cols);
      const end = Math.max(start + 1, Math.floor(((c + 1) * bandCount) / this.cols));
      let peak = 0;
      for (let b = start; b < end && b < bandCount; b++) {
        if (bands[b] > peak) peak = bands[b];
      }
      this.targets[c] = Math.max(0, Math.min(1, peak));
    }
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    const dtSec = Math.max(0, Math.min(dtMs, 100)) / 1000;
    const attack = 1 - Math.exp(-ATTACK_RATE * dtSec);
    const decay = 1 - Math.exp(-DECAY_RATE * dtSec);
    let anyActive = false;

    for (let c = 0; c < this.cols; c++) {
      const target = this.targets[c];
      const cur = this.current[c];
      const next = cur + (target - cur) * (target >= cur ? attack : decay);
      this.current[c] = next < EPSILON && target < EPSILON ? 0 : next;
      if (this.current[c] > EPSILON || target > EPSILON) anyActive = true;
    }

    // Write live smoothed levels into the newest front slice.
    const frontOffset = this.head * this.cols;
    for (let c = 0; c < this.cols; c++) {
      this.history[frontOffset + c] = this.current[c];
    }

    let anyHistory = false;
    if (this.depth > 1 && !reducedMotion) {
      for (let i = 0; i < this.history.length; i++) {
        if (this.history[i] > EPSILON) {
          anyHistory = true;
          break;
        }
      }
    }

    if (this.depth > 1) {
      if (reducedMotion) {
        // Under reduced motion, hold history flat so only the front slice updates.
        this.snapshotElapsedMs = 0;
        for (let d = 0; d < this.depth; d++) {
          if (d === this.head) continue;
          this.history.fill(0, d * this.cols, (d + 1) * this.cols);
        }
      } else {
        this.snapshotElapsedMs += Math.max(0, Math.min(dtMs, 100));
        if (this.snapshotElapsedMs >= SNAPSHOT_MS) {
          this.snapshotElapsedMs -= SNAPSHOT_MS;
          if (anyActive || anyHistory || this.playing) {
            this.head = (this.head + 1) % this.depth;
            const newFront = this.head * this.cols;
            for (let c = 0; c < this.cols; c++) {
              this.history[newFront + c] = this.current[c];
            }
          }
        }
      }
    }

    return anyActive || anyHistory;
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    ctx.clearRect(0, 0, width, height);

    if (width !== this.fitWidth || height !== this.fitHeight) {
      this.view.fit(width, height, 0, this.cols, 0, this.depth, 0, this.maxCubes);
      this.fitWidth = width;
      this.fitHeight = height;
    }

    const p = this.palette;
    this.view.plane(ctx, 0, 0, this.cols, this.depth, 0, p.floor);

    if (options.preview) {
      const frontY = this.depth - 1;
      for (let x = 0; x < this.cols; x++) {
        const sample = PREVIEW_PROFILE[x % PREVIEW_PROFILE.length];
        const cubes = Math.max(1, Math.round(sample * this.maxCubes));
        for (let z = 0; z < cubes; z++) {
          const faces = z === cubes - 1 ? p.crest : p.slices[this.depth - 1];
          const mask = (z === cubes - 1 ? FACE_TOP : 0) | FACE_LEFT | FACE_RIGHT;
          this.view.cube(ctx, x, frontY, z, 1, faces, mask);
        }
      }
      return;
    }

    // Draw back-to-front: increasing x, then increasing y (oldest -> newest), then z.
    for (let x = 0; x < this.cols; x++) {
      for (let d = 0; d < this.depth; d++) {
        // Map d=0..depth-1 from oldest slice to newest (this.head).
        const sliceIdx = (this.head + 1 + d) % this.depth;
        const level = this.history[sliceIdx * this.cols + x];
        const cubes = Math.min(this.maxCubes, Math.round(level * this.maxCubes));
        if (cubes <= 0) continue;
        const isFront = d === this.depth - 1;
        const sliceFaces = p.slices[d];
        for (let z = 0; z < cubes; z++) {
          const isTop = z === cubes - 1;
          const faces = isFront && isTop ? p.crest : sliceFaces;
          const mask = isTop ? FACE_ALL : FACE_LEFT | FACE_RIGHT;
          this.view.cube(ctx, x, d, z, 1, faces, mask);
        }
      }
    }
  }
}

/** Isometric 3D voxel waterfall spectrum powered by `IsoView`. */
export const VOXEL_ISO_PRESET: VisualizerPreset = {
  id: "voxel-iso",
  labelKey: "visualizers.voxelIso",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return new VoxelIsoInstance(size);
  },
};
