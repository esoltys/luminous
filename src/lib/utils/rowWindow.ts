// Layout math for drawing only the on-screen part of a long list whose row
// heights are known up front (SongTable, #1898). DOM-free so it can be tested
// directly; the component supplies the heights and the visible span.

/** Where each row starts: `tops[i]` is row i's top edge and `tops[n]` the list's total height. */
export function rowTops(heights: readonly number[]): number[] {
  const tops = new Array<number>(heights.length + 1);
  tops[0] = 0;
  for (let i = 0; i < heights.length; i++) tops[i + 1] = tops[i] + heights[i];
  return tops;
}

/**
 * The rows `[start, end)` that overlap the span `[top, bottom)` (in list
 * coordinates), widened by `overscan` rows on each side. A span outside the
 * list yields an empty or clamped range, never an out-of-bounds one.
 */
export function visibleRows(
  tops: readonly number[],
  top: number,
  bottom: number,
  overscan: number
): { start: number; end: number } {
  const n = tops.length - 1;
  // First row whose bottom edge is below `top`.
  let lo = 0;
  let hi = n;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (tops[mid + 1] <= top) lo = mid + 1;
    else hi = mid;
  }
  let end = lo;
  while (end < n && tops[end] < bottom) end++;
  return { start: Math.max(0, lo - overscan), end: Math.min(n, end + overscan) };
}
