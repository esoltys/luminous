/**
 * Tile-grid math shared by CoverMosaic.svelte and the share card's
 * buildMosaicCoverHtml, so the two can't drift (#1496).
 *
 * The mosaic is a grid of square unit cells: the big tile (hero image or
 * covers[0]) spans the top-left NxN cells (N = 2, or 3 when that fills a grid
 * exactly, e.g. a Top 10 as 1 big + 3x3) and every other cover is one unit
 * cell, auto-placed row-major in the cells that are left (to the right of the
 * big tile first, then under it once there are more rows than the big tile).
 */

export interface MosaicLayoutOptions {
  /** Box to fill, in px. `Infinity` for an unconstrained axis. */
  width: number;
  height: number;
  /** Covers beyond the big tile. */
  quarterCount: number;
  gap?: number;
  /** Smallest unit-cell edge worth drawing. */
  minTile?: number;
  maxRows?: number;
  maxCols?: number;
}

export interface MosaicLayout {
  cols: number;
  rows: number;
  /** Big tile edge in unit cells (2 or 3); it is `heroSpan * unit + (heroSpan - 1) * gap` px. */
  heroSpan: number;
  /** Unit-cell edge in px. */
  unit: number;
  gap: number;
  /** Rendered size of the whole grid; never larger than the box. */
  width: number;
  height: number;
  /** Quarter covers drawn; the rest of the grid's cells stay empty. */
  shown: number;
}

export const MOSAIC_GAP = 2;
/** The bounds that reproduce the original fixed layout (1 big + up to 4 quarters). */
const LEGACY_MAX_ROWS = 2;
const LEGACY_MAX_COLS = 4;

/**
 * Picks the grid that shows the most covers inside the box, then the biggest tiles,
 * discounting grids with empty cells (a dangling gap reads as broken). Returns `null` when
 * there are no quarter covers (the big tile stands alone) or nothing satisfies
 * `minTile`.
 */
export function computeMosaicLayout(opts: MosaicLayoutOptions): MosaicLayout | null {
  const { width, height, quarterCount } = opts;
  const gap = opts.gap ?? MOSAIC_GAP;
  const minTile = opts.minTile ?? 0;
  const maxRows = opts.maxRows ?? LEGACY_MAX_ROWS;
  const maxCols = opts.maxCols ?? LEGACY_MAX_COLS;
  if (quarterCount <= 0) return null;

  let best: MosaicLayout | null = null;
  let bestEmpty = 0;
  // Effective size: each empty cell discounts a grid by 35%, so an exact fill
  // wins unless a gapped grid has much bigger tiles.
  const effective = (unit: number, empty: number) => unit / (1 + 0.35 * empty);
  for (let heroSpan = 2; heroSpan <= 3; heroSpan++) {
    for (let rows = heroSpan; rows <= maxRows; rows++) {
      for (let cols = heroSpan + 1; cols <= maxCols; cols++) {
        const slots = cols * rows - heroSpan * heroSpan;
        const unit = Math.min((width - (cols - 1) * gap) / cols, (height - (rows - 1) * gap) / rows);
        if (!(unit > 0) || unit < minTile) continue;
        const shown = Math.min(slots, quarterCount);
        const empty = slots - shown;
        const better =
          !best ||
          shown > best.shown ||
          (shown === best.shown &&
            (effective(unit, empty) > effective(best.unit, bestEmpty) + 0.5 ||
              (Math.abs(effective(unit, empty) - effective(best.unit, bestEmpty)) <= 0.5 && empty < bestEmpty)));
        if (better) {
          best = {
            cols,
            rows,
            heroSpan,
            unit,
            gap,
            width: cols * unit + (cols - 1) * gap,
            height: rows * unit + (rows - 1) * gap,
            shown,
          };
          bestEmpty = empty;
        }
      }
    }
  }
  return best;
}
