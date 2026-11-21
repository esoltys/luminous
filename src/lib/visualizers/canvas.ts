/**
 * Prepares a `<canvas>` for HiDPI 2D rendering at its current CSS layout size,
 * resizing the backing store only when dimensions or `devicePixelRatio` change
 * and resetting the transform matrix idempotently via `setTransform`.
 * Returns `null` if the canvas is detached, zero-sized, or lacks a 2D context.
 */
export function prepareCanvas2d(
  canvas: HTMLCanvasElement | null,
): { ctx: CanvasRenderingContext2D; width: number; height: number } | null {
  if (!canvas) return null;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  if (width <= 0 || height <= 0) return null;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;

  const dpr = (typeof window !== "undefined" && window.devicePixelRatio) || 1;
  const targetW = Math.round(width * dpr);
  const targetH = Math.round(height * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { ctx, width, height };
}
