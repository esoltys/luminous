// Layered-ellipse radial gradient generator.
//
// Reverse-engineered from the SVG output of Justin Jay Wang's "Layered radial"
// method (used on openai.com 2020-2022): https://justinjay.wang/methods-for-random-gradients/
// Each palette color gets its own radial gradient (solid color fading to
// transparent). A full-canvas rect filled with that gradient is then warped
// with an SVG transform (scale -> skew -> rotate -> translate) so the
// gradient's circular falloff renders as a positioned, rotated ellipse.
// Stacking several of these per color, then several colors, composites into
// an organic blob gradient — fully vector, and cheap to serve.

export interface EllipseGradientOptions {
  /** Rendered <svg> width attribute. Defaults to `size`. */
  width?: number;
  /** Rendered <svg> height attribute. Defaults to `size`. */
  height?: number;
  /** Square coordinate space the ellipses are laid out and transformed in. */
  size?: number;
  /** Palette to draw ellipses from. Defaults to a 4-color built-in set. */
  colors?: string[];
  /** How many ellipse layers to stack per color. */
  layersPerColor?: number;
  /** Deterministic seed — same seed + options always produce the same SVG. */
  seed?: number | string;
  /** CSS `saturate()` filter percentage applied to the whole gradient. */
  saturation?: number;
}

const DEFAULT_COLORS = ["#5135FF", "#FF5828", "#F69CFF", "#FFA50F"];
const DEFAULT_SIZE = 600;
const DEFAULT_LAYERS_PER_COLOR = 3;
const DEFAULT_SATURATION = 125;

/** Deterministic PRNG (mulberry32) so a given seed always reproduces the same gradient. */
function createRng(seed?: number | string): () => number {
  if (seed === undefined) return Math.random;

  let state = typeof seed === "number" ? seed >>> 0 : hashString(seed);
  return () => {
    state |= 0;
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** FNV-1a string hash, used to seed the PRNG from a string (e.g. a song/playlist id). */
function hashString(value: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i++) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

function randRange(rng: () => number, min: number, max: number): number {
  return min + rng() * (max - min);
}

/** Generates a self-contained, unique-per-call layered-ellipse gradient SVG string. */
export function generateEllipseGradientSvg(options: EllipseGradientOptions = {}): string {
  const size = options.size ?? DEFAULT_SIZE;
  const width = options.width ?? size;
  const height = options.height ?? size;
  const colors = options.colors && options.colors.length > 0 ? options.colors : DEFAULT_COLORS;
  const layersPerColor = options.layersPerColor ?? DEFAULT_LAYERS_PER_COLOR;
  const saturation = options.saturation ?? DEFAULT_SATURATION;
  const rng = createRng(options.seed);

  // Random per-call/per-seed prefix so multiple gradients inlined on the same
  // page never collide on gradient element ids.
  const runId = Math.floor(randRange(rng, 0, 1e9)).toString(36);
  const center = size / 2;
  const backgroundColor = colors[Math.floor(randRange(rng, 0, colors.length))];

  const gradientDefs: string[] = [];
  const layerRects: string[] = [];
  const layerOrder: { color: string; gradientId: string }[] = [];

  colors.forEach((color, colorIndex) => {
    for (let layer = 0; layer < layersPerColor; layer++) {
      const gradientId = `eg-${runId}-${colorIndex}-${layer}`;
      const fx = randRange(rng, 0.15, 0.85);
      const fy = randRange(rng, 0.3, 0.7);

      gradientDefs.push(
        `<radialGradient id="${gradientId}" fx="${fx.toFixed(4)}" fy="${fy.toFixed(4)}">` +
          `<stop offset="0%" stop-color="${color}"/>` +
          `<stop offset="100%" stop-color="${color}" stop-opacity="0"/>` +
        `</radialGradient>`
      );
      layerOrder.push({ color, gradientId });
    }
  });

  // Shuffle layer order (Fisher-Yates) so colors interleave instead of
  // stacking in solid color-by-color bands.
  for (let i = layerOrder.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [layerOrder[i], layerOrder[j]] = [layerOrder[j], layerOrder[i]];
  }

  for (const { gradientId } of layerOrder) {
    const scaleX = randRange(rng, 0.8, 1.5);
    const scaleY = randRange(rng, 0.8, 1.5);
    const skew = randRange(rng, -10, 10);
    const rotate = randRange(rng, 0, 360);
    const offsetRange = size * 0.38;
    const dx = randRange(rng, -offsetRange, offsetRange);
    const dy = randRange(rng, -offsetRange, offsetRange);

    layerRects.push(
      `<rect x="0" y="0" width="100%" height="100%" fill="url(#${gradientId})" ` +
        `transform="translate(${center} ${center}) scale(${scaleX.toFixed(4)} ${scaleY.toFixed(4)}) ` +
        `skewX(${skew.toFixed(4)}) rotate(${rotate.toFixed(4)}) translate(${dx.toFixed(4)} ${dy.toFixed(4)}) ` +
        `translate(${-center} ${-center})"/>`
    );
  }

  return (
    `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" ` +
    `width="${width}" height="${height}" viewBox="0 0 ${size} ${size}" ` +
    `style="filter:saturate(${saturation}%)" preserveAspectRatio="xMidYMid slice">` +
      `<defs>${gradientDefs.join("")}</defs>` +
      `<rect x="0" y="0" width="100%" height="100%" fill="${backgroundColor}"/>` +
      layerRects.join("") +
    `</svg>`
  );
}

/** Convenience wrapper for use as a CSS `background-image` value. */
export function ellipseGradientDataUri(options?: EllipseGradientOptions): string {
  const svg = generateEllipseGradientSvg(options);
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}
