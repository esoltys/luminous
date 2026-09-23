/**
 * In-place separable box blur over RGBA pixel data; three passes approximate
 * a Gaussian with sigma ≈ radius. Edges clamp. Used on tiny (e.g. 64×64)
 * images only — see BlurredCover.svelte — so a plain O(w·h·passes) loop is
 * plenty; WebKitGTK has no CanvasRenderingContext2D.filter to do it instead.
 */
export function boxBlurRGBA(data: Uint8ClampedArray, width: number, height: number, radius: number, passes = 3): void {
  if (radius < 1 || width === 0 || height === 0) return;
  const scratch = new Uint8ClampedArray(data.length);
  for (let pass = 0; pass < passes; pass++) {
    blurAxis(data, scratch, width, height, radius, true);
    blurAxis(scratch, data, width, height, radius, false);
  }
}

function blurAxis(
  src: Uint8ClampedArray,
  dst: Uint8ClampedArray,
  width: number,
  height: number,
  radius: number,
  horizontal: boolean
): void {
  const lines = horizontal ? height : width;
  const length = horizontal ? width : height;
  const span = radius * 2 + 1;
  const index = (line: number, pos: number) =>
    (horizontal ? line * width + pos : pos * width + line) * 4;
  const clamp = (pos: number) => Math.min(length - 1, Math.max(0, pos));

  for (let line = 0; line < lines; line++) {
    for (let channel = 0; channel < 4; channel++) {
      let sum = 0;
      for (let k = -radius; k <= radius; k++) {
        sum += src[index(line, clamp(k)) + channel];
      }
      for (let pos = 0; pos < length; pos++) {
        dst[index(line, pos) + channel] = Math.round(sum / span);
        sum += src[index(line, clamp(pos + radius + 1)) + channel] - src[index(line, clamp(pos - radius)) + channel];
      }
    }
  }
}
