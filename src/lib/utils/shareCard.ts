// Social share card builder (#97). The card is composed as a single SVG
// string — the same layered-ellipse gradient used by the immersive view for
// the background, with an <foreignObject> overlay for the actual HTML/CSS
// content (cover art, title, metadata, track list) — then rasterized to a
// PNG by loading that SVG into an <img> and drawing it onto a canvas. This
// avoids pulling in a DOM-to-image dependency: the whole card only ever
// exists as markup we generate, never a live component tree we'd need to
// snapshot.

import { generateEllipseGradientSvg } from "./ellipseGradient";
import exposeFontUrl from "../fonts/expose/expose-700.woff2?url";

// The mark's colors are fixed brand values (matching static/luminous-mark.svg
// and the app icon) — unlike the card's text, it doesn't adapt to the
// light/dark card theme.
const LUMINOUS_MARK_SVG = (size: number) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="${size}" height="${size}" style="flex-shrink:0;">` +
    `<circle cx="100" cy="100" r="77" fill="none" stroke="#626FE8" stroke-width="14"/>` +
    `<circle cx="100" cy="100" r="92" fill="none" stroke="#FFB648" stroke-width="8"/>` +
    `<circle cx="100" cy="100" r="68" fill="#0A0A0D"/>` +
    `<circle cx="152" cy="57" r="11" fill="#FFFFFF"/>` +
  `</svg>`;

export type ShareAspectRatio = "1:1" | "9:16" | "16:9" | "4:3" | "3:4";

export const SHARE_ASPECT_RATIOS: { id: ShareAspectRatio; width: number; height: number }[] = [
  { id: "1:1", width: 1080, height: 1080 },
  { id: "9:16", width: 1080, height: 1920 },
  { id: "16:9", width: 1920, height: 1080 },
  { id: "4:3", width: 1440, height: 1080 },
  { id: "3:4", width: 1080, height: 1440 },
];

export type ShareCardTheme = "light" | "dark";

export interface ShareCardTrack {
  number?: number | null;
  title: string;
}

export interface ShareCardOptions {
  aspectRatio: ShareAspectRatio;
  theme: ShareCardTheme;
  seed: string;
  backgroundColors?: string[];
  coverDataUri: string | null;
  /** Up to 4 cover data URIs, front-to-back, rendered as a fanned stack
   * (mirroring CoverStack.svelte's "right" direction transform) instead of
   * the single `coverDataUri` image — used for playlist cards, where a
   * single cover would misrepresent a multi-artist/multi-album mix.
   * Ignored when it has fewer than 2 entries; falls back to `coverDataUri`. */
  coverStackDataUris?: (string | null)[] | null;
  title: string;
  subtitle: string;
  metadataLine: string;
  tracks?: ShareCardTrack[];
  includeTrackList: boolean;
  /** Base64 data URI of the Expose wordmark font, embedded as a self-contained
   * @font-face so the footer renders in-brand once rasterized — foreignObject
   * content only sees fonts declared inside the image itself, not the host
   * document's stylesheets. Fetched automatically by rasterizeShareCard();
   * omit (e.g. in tests) to fall back to the sans-serif stack. */
  exposeFontDataUri?: string | null;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * Track list is laid out as CSS columns so a long tracklist fans out
 * sideways instead of forcing one tall, mostly-empty-feeling column —
 * landscape frames have the width to spare for a third column, portrait/
 * square ones cap out at two.
 */
function trackListLayout(dims: { width: number; height: number }, trackCount: number): { columns: number; maxVisible: number } {
  const isPortrait = dims.height > dims.width;
  const isSquareish = Math.abs(dims.width - dims.height) < dims.width * 0.15;
  const rowsPerColumn = isPortrait ? 10 : isSquareish ? 7 : 6;
  const maxColumns = isPortrait || isSquareish ? 2 : 3;
  const columns = Math.min(maxColumns, Math.max(1, Math.ceil(trackCount / rowsPerColumn)));
  return { columns, maxVisible: columns * rowsPerColumn };
}

/**
 * Renders either a single cover image or, when `stackUris` has 2+ entries, a
 * fanned stack of up to 4 — same offset/rotation/scale/opacity progression as
 * CoverStack.svelte's directional transforms (`translate(i*7, i*-5)
 * rotate(i*5deg) scale(1-i*0.05)`), expressed as a fraction of `size` so it
 * holds up at any card resolution. Painted back-to-front in DOM order so the
 * front cover (index 0) needs no explicit z-index.
 *
 * `fanLeft` mirrors the horizontal offset/rotation so the stack fans away
 * from, rather than into, the text column sitting beside it in landscape
 * layouts — the cover sits on the left with text to its right, so fanning
 * further right ran the back tiles under the title/metadata text.
 */
function buildCoverHtml(
  coverDataUri: string | null,
  stackUris: (string | null)[] | null | undefined,
  size: number,
  fanLeft = false
): string {
  const stack = (stackUris ?? []).filter((u): u is string => !!u).slice(0, 4);
  if (stack.length >= 2) {
    const radius = Math.round(size * 0.06);
    const dxSign = fanLeft ? -1 : 1;
    const tiles = stack
      .map((uri, i) => {
        const dx = Math.round(i * size * 0.073 * dxSign);
        const dy = Math.round(i * size * -0.052);
        const rot = i * 5 * dxSign;
        const scale = 1 - i * 0.05;
        const opacity = 1 - i * 0.09;
        return `<img src="${uri}" style="position:absolute;inset:0;width:100%;height:100%;object-fit:cover;border-radius:${radius}px;box-shadow:0 20px 50px rgba(0,0,0,0.4);opacity:${opacity};transform:translate(${dx}px,${dy}px) rotate(${rot}deg) scale(${scale});" />`;
      })
      .reverse();
    return `<div style="position:relative;width:${size}px;height:${size}px;flex-shrink:0;">${tiles.join("")}</div>`;
  }
  const single = coverDataUri ?? stack[0] ?? null;
  return single
    ? `<img src="${single}" style="width:${size}px;height:${size}px;object-fit:cover;border-radius:${Math.round(size * 0.06)}px;box-shadow:0 20px 50px rgba(0,0,0,0.4);flex-shrink:0;" />`
    : "";
}

export function buildShareCardSvg(options: ShareCardOptions): { svg: string; width: number; height: number } {
  const dims = SHARE_ASPECT_RATIOS.find((r) => r.id === options.aspectRatio) ?? SHARE_ASPECT_RATIOS[0];
  const { width, height } = dims;
  // "Dark" means the card itself reads dark (a darkening scrim, light text);
  // "light" means the card reads light (a brightening scrim, dark text) —
  // the opposite pairing looked backwards against the sun/moon icons.
  const isDark = options.theme === "dark";
  const textPrimary = isDark ? "#f5f6f8" : "#0b0c0f";
  const textSecondary = isDark ? "rgba(245,246,248,0.78)" : "rgba(11,12,15,0.72)";
  const scrimFrom = isDark ? "rgba(0,0,0,0)" : "rgba(255,255,255,0)";
  const scrimTo = isDark ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.55)";
  const cardPad = Math.round(width * 0.06);
  // Portrait/square frames stack cover-then-text centered in the middle of
  // the canvas (a bigger cover, since there's little horizontal room);
  // landscape frames keep a side-by-side row so the wide aspect isn't mostly
  // empty gradient either side of a narrow text column.
  const isPortrait = height >= width;
  // A very elongated portrait frame (9:16) has a lot more vertical room than
  // a mild one (3:4) at the same width, so content sized purely off width
  // reads small and leaves dead space top and bottom. Scale content up
  // relative to how much taller the frame is than a baseline 3:4 (1.33:1).
  const elongation = height / width;
  const contentScale = isPortrait ? Math.min(1.5, Math.max(1, elongation / 1.33)) : 1;
  // Without a track list the text block is just as many short lines as the
  // card actually has (title, plus an optional subtitle and metadata line —
  // album cards show all three, artist/playlist cards with their toggles off
  // may show only the title), so the cover can claim a lot more of the frame
  // when there's a full text block to sit next to than when there's almost
  // none — size each variant for what it's actually sitting next to rather
  // than one flat ratio for every card.
  const willShowTrackList = !!(options.includeTrackList && options.tracks && options.tracks.length > 0);
  const textLineCount = 1 + (options.subtitle ? 1 : 0) + (options.metadataLine ? 1 : 0);
  // The elongation boost exists to fill extra vertical room with more text —
  // when there's barely any text to begin with, cap how much of that boost
  // the cover itself absorbs so it doesn't balloon to fill the empty space
  // (e.g. a 9:16 artist card with everything shown as just a name).
  const coverContentScale = textLineCount >= 3 ? contentScale : Math.min(contentScale, 1.15);
  const noTrackListPortraitFraction = textLineCount >= 3 ? 0.72 : textLineCount === 2 ? 0.62 : 0.5;
  const noTrackListLandscapeFraction = textLineCount >= 3 ? 0.6 : textLineCount === 2 ? 0.54 : 0.46;
  const coverSize = Math.round(
    isPortrait
      ? width * (willShowTrackList ? 0.56 : noTrackListPortraitFraction) * coverContentScale
      : Math.min(width, height) * (willShowTrackList ? 0.46 : noTrackListLandscapeFraction)
  );

  const background = generateEllipseGradientSvg({
    width,
    height,
    colors: options.backgroundColors,
    seed: options.seed,
  });
  // Strip the outer <svg ...> wrapper so it can be inlined as this card's own background layer.
  const backgroundInner = background.replace(/^<svg[^>]*>/, "").replace(/<\/svg>$/, "");

  let trackListHtml = "";
  if (willShowTrackList && options.tracks) {
    const { columns, maxVisible } = trackListLayout(dims, options.tracks.length);
    const visible = options.tracks.slice(0, maxVisible);
    const overflow = options.tracks.length - visible.length;
    const rowFontSize = Math.round(width * 0.017 * contentScale);

    const rows = visible.map(
      (track) =>
        `<div style="display:flex;gap:10px;align-items:baseline;padding:4px 0;font-size:${rowFontSize}px;color:${textSecondary};break-inside:avoid;">` +
          (track.number != null
            ? `<span style="min-width:1.8em;text-align:right;opacity:0.7;">${track.number}</span>`
            : "") +
          `<span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(track.title)}</span>` +
        `</div>`
    );
    const overflowRow =
      overflow > 0
        ? `<div style="column-span:all;padding:4px 0;font-size:${rowFontSize}px;color:${textSecondary};opacity:0.7;">+${overflow} more</div>`
        : "";

    // Belt-and-suspenders against a pathological combination (a very long,
    // two-line-wrapped title plus a huge box-set tracklist): even though
    // trackListLayout already sizes for the expected case, cap the block's
    // own height and clip it so it can never grow past the LUMINOUS mark
    // pinned near the bottom of the frame, rather than overlapping it.
    const trackListMaxHeight = Math.round(height * (isPortrait ? 0.3 : 0.4));

    trackListHtml =
      `<div style="margin-top:${Math.round(width * 0.025 * contentScale)}px;text-align:left;width:100%;column-count:${columns};column-gap:${Math.round(width * 0.03)}px;max-height:${trackListMaxHeight}px;overflow:hidden;">` +
        rows.join("") + overflowRow +
      `</div>`;
  }

  const showTrackList = trackListHtml.length > 0;
  const textAlign = isPortrait ? "center" : "left";
  const groupDirection = isPortrait ? "column" : "row";
  const textBlockMaxWidth = isPortrait ? Math.round(width * 0.82) : undefined;

  const contentHtml = `
    <div xmlns="http://www.w3.org/1999/xhtml" style="position:relative;width:100%;height:100%;overflow:hidden;display:flex;flex-direction:column;align-items:center;justify-content:center;padding:${cardPad}px;box-sizing:border-box;font-family:'Inter','Segoe UI',system-ui,sans-serif;">
      <div style="display:flex;flex-direction:${groupDirection};align-items:center;gap:${Math.round(width * 0.035 * contentScale)}px;max-width:100%;">
        ${buildCoverHtml(options.coverDataUri, options.coverStackDataUris, coverSize, !isPortrait)}
        <div style="min-width:0;${isPortrait ? "" : "flex:1;"}display:flex;flex-direction:column;gap:2px;align-items:${isPortrait ? "center" : "flex-start"};text-align:${textAlign};${textBlockMaxWidth ? `max-width:${textBlockMaxWidth}px;` : ""}">
          <div style="font-size:${Math.round(width * 0.046 * contentScale)}px;font-weight:800;color:${textPrimary};line-height:1.3;padding-bottom:0.08em;overflow:hidden;text-overflow:ellipsis;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;">${escapeHtml(options.title)}</div>
          <div style="font-size:${Math.round(width * 0.026 * contentScale)}px;font-weight:600;color:${textSecondary};margin-top:6px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:100%;">${escapeHtml(options.subtitle)}</div>
          <div style="font-size:${Math.round(width * 0.019 * contentScale)}px;color:${textSecondary};margin-top:6px;">${escapeHtml(options.metadataLine)}</div>
          ${showTrackList ? trackListHtml : ""}
        </div>
      </div>
      <div style="position:absolute;left:${cardPad}px;bottom:${cardPad}px;display:flex;align-items:center;gap:${Math.round(width * 0.008)}px;opacity:0.85;">
        ${LUMINOUS_MARK_SVG(Math.round(width * 0.024))}
        <span style="font-family:'Expose','Inter','Segoe UI',system-ui,sans-serif;font-size:${Math.round(width * 0.015)}px;font-weight:700;letter-spacing:0.04em;color:${textSecondary};">LUMINOUS</span>
      </div>
    </div>
  `;

  const fontFace = options.exposeFontDataUri
    ? `<style>@font-face{font-family:'Expose';src:url(${options.exposeFontDataUri}) format('woff2');font-weight:700;font-style:normal;}</style>`
    : "";

  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">` +
      `<g>${backgroundInner}</g>` +
      `<defs>${fontFace}<linearGradient id="scrim" x1="0" y1="0" x2="0" y2="1">` +
        `<stop offset="0%" stop-color="${scrimFrom}"/>` +
        `<stop offset="100%" stop-color="${scrimTo}"/>` +
      `</linearGradient></defs>` +
      `<rect x="0" y="0" width="${width}" height="${height}" fill="url(#scrim)"/>` +
      `<foreignObject x="0" y="0" width="${width}" height="${height}">${contentHtml}</foreignObject>` +
    `</svg>`;

  return { svg, width, height };
}

interface StatsShareCardItem {
  label: string;
  secondary?: string | null;
}

export interface StatsShareCardSection {
  title: string;
  /** Pre-capped by the caller (e.g. top 5) — this builder renders whatever it's given. */
  items: StatsShareCardItem[];
}

export interface StatsShareCardClockBucket {
  label: string;
  count: number;
}

export interface StatsShareCardOptions {
  aspectRatio: ShareAspectRatio;
  theme: ShareCardTheme;
  seed: string;
  backgroundColors?: string[];
  rangeLabel: string;
  totalMinutesLabel: string;
  /** Top Artists/Albums/Songs/Genres, in that order, laid out as a 2x2 grid. */
  sections: StatsShareCardSection[];
  /** Morning/Afternoon/Evening/Late Night, in that order. */
  clockBuckets: StatsShareCardClockBucket[];
  exposeFontDataUri?: string | null;
}

/**
 * A distinct "Wrapped"-style summary card — not a single-entity card, so it
 * doesn't reuse buildShareCardSvg's cover/title/tracklist layout. Shares the
 * same background gradient, scrim, and LUMINOUS footer mark for visual
 * consistency with the entity cards.
 */
export function buildStatsShareCardSvg(options: StatsShareCardOptions): { svg: string; width: number; height: number } {
  const dims = SHARE_ASPECT_RATIOS.find((r) => r.id === options.aspectRatio) ?? SHARE_ASPECT_RATIOS[0];
  const { width, height } = dims;
  const isDark = options.theme === "dark";
  const textPrimary = isDark ? "#f5f6f8" : "#0b0c0f";
  const textSecondary = isDark ? "rgba(245,246,248,0.78)" : "rgba(11,12,15,0.72)";
  const textTertiary = isDark ? "rgba(245,246,248,0.55)" : "rgba(11,12,15,0.5)";
  const cardBg = isDark ? "rgba(0,0,0,0.28)" : "rgba(255,255,255,0.4)";
  const scrimFrom = isDark ? "rgba(0,0,0,0)" : "rgba(255,255,255,0)";
  const scrimTo = isDark ? "rgba(0,0,0,0.55)" : "rgba(255,255,255,0.55)";
  const pad = Math.round(width * 0.055);

  const background = generateEllipseGradientSvg({ width, height, colors: options.backgroundColors, seed: options.seed });
  const backgroundInner = background.replace(/^<svg[^>]*>/, "").replace(/<\/svg>$/, "");

  const titleSize = Math.round(width * 0.038);
  const subtitleSize = Math.round(width * 0.02);
  const sectionTitleSize = Math.round(width * 0.02);
  const rowSize = Math.round(width * 0.0165);
  const clockLabelSize = Math.round(width * 0.014);

  const sectionsHtml = options.sections
    .map((section) => {
      const rows = section.items
        .map(
          (item, i) =>
            `<div style="display:flex;gap:8px;align-items:baseline;padding:3px 0;font-size:${rowSize}px;color:${textSecondary};">` +
              `<span style="min-width:1.6em;opacity:0.6;">${i + 1}</span>` +
              `<span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1;">${escapeHtml(item.label)}${
                item.secondary ? ` <span style="opacity:0.65;">— ${escapeHtml(item.secondary)}</span>` : ""
              }</span>` +
            `</div>`
        )
        .join("");
      return (
        `<div style="background:${cardBg};border-radius:${Math.round(width * 0.014)}px;padding:${Math.round(width * 0.018)}px;min-width:0;">` +
          `<div style="font-size:${sectionTitleSize}px;font-weight:800;color:${textPrimary};margin-bottom:4px;">${escapeHtml(section.title)}</div>` +
          rows +
        `</div>`
      );
    })
    .join("");

  const maxClockCount = Math.max(1, ...options.clockBuckets.map((b) => b.count));
  const clockBarMaxHeight = Math.round(height * 0.09);
  const clockHtml = `
    <div style="display:flex;align-items:flex-end;justify-content:center;gap:${Math.round(width * 0.035)}px;margin-top:${Math.round(width * 0.02)}px;">
      ${options.clockBuckets
        .map((bucket) => {
          const barHeight = Math.max(4, Math.round((bucket.count / maxClockCount) * clockBarMaxHeight));
          return (
            `<div style="display:flex;flex-direction:column;align-items:center;gap:4px;">` +
              `<div style="width:${Math.round(width * 0.028)}px;height:${clockBarMaxHeight}px;display:flex;align-items:flex-end;">` +
                `<div style="width:100%;height:${barHeight}px;border-radius:${Math.round(width * 0.006)}px;background:${textPrimary};opacity:0.75;"></div>` +
              `</div>` +
              `<span style="font-size:${clockLabelSize}px;color:${textTertiary};">${escapeHtml(bucket.label)}</span>` +
            `</div>`
          );
        })
        .join("")}
    </div>
  `;

  const contentHtml = `
    <div xmlns="http://www.w3.org/1999/xhtml" style="position:relative;width:100%;height:100%;overflow:hidden;display:flex;flex-direction:column;align-items:center;justify-content:center;padding:${pad}px;box-sizing:border-box;font-family:'Inter','Segoe UI',system-ui,sans-serif;">
      <div style="font-size:${titleSize}px;font-weight:800;color:${textPrimary};text-align:center;">${escapeHtml(options.rangeLabel)}</div>
      <div style="font-size:${subtitleSize}px;font-weight:600;color:${textSecondary};margin-top:4px;margin-bottom:${Math.round(width * 0.03)}px;text-align:center;">${escapeHtml(options.totalMinutesLabel)}</div>
      <div style="display:grid;grid-template-columns:1fr 1fr;gap:${Math.round(width * 0.018)}px;width:100%;max-width:${Math.round(width * 0.86)}px;">
        ${sectionsHtml}
      </div>
      ${clockHtml}
      <div style="position:absolute;left:${pad}px;bottom:${pad}px;display:flex;align-items:center;gap:${Math.round(width * 0.008)}px;opacity:0.85;">
        ${LUMINOUS_MARK_SVG(Math.round(width * 0.024))}
        <span style="font-family:'Expose','Inter','Segoe UI',system-ui,sans-serif;font-size:${Math.round(width * 0.015)}px;font-weight:700;letter-spacing:0.04em;color:${textSecondary};">LUMINOUS</span>
      </div>
    </div>
  `;

  const fontFace = options.exposeFontDataUri
    ? `<style>@font-face{font-family:'Expose';src:url(${options.exposeFontDataUri}) format('woff2');font-weight:700;font-style:normal;}</style>`
    : "";

  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">` +
      `<g>${backgroundInner}</g>` +
      `<defs>${fontFace}<linearGradient id="scrim" x1="0" y1="0" x2="0" y2="1">` +
        `<stop offset="0%" stop-color="${scrimFrom}"/>` +
        `<stop offset="100%" stop-color="${scrimTo}"/>` +
      `</linearGradient></defs>` +
      `<rect x="0" y="0" width="${width}" height="${height}" fill="url(#scrim)"/>` +
      `<foreignObject x="0" y="0" width="${width}" height="${height}">${contentHtml}</foreignObject>` +
    `</svg>`;

  return { svg, width, height };
}

/** Loads an image, tolerating cross-origin/blocked sources by resolving with `null` instead of rejecting. */
async function loadImage(src: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => resolve(null);
    img.src = src;
  });
}

/** Converts an arbitrary image URL (including Tauri asset URLs) to a data URI so it can be safely embedded in an SVG foreignObject and rasterized without tainting the canvas. */
export async function toDataUri(url: string): Promise<string | null> {
  try {
    const response = await fetch(url);
    const blob = await response.blob();
    return await new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });
  } catch {
    return null;
  }
}

let cachedExposeFontDataUri: Promise<string | null> | null = null;

/** Fetches and caches the Expose wordmark font as a data URI (see ShareCardOptions.exposeFontDataUri). */
function getExposeFontDataUri(): Promise<string | null> {
  if (!cachedExposeFontDataUri) {
    cachedExposeFontDataUri = toDataUri(exposeFontUrl);
  }
  return cachedExposeFontDataUri;
}

/** Loads a built card SVG into an <img> and rasterizes it to a PNG blob at `scale`x the card's declared pixel size. Shared by rasterizeShareCard() and rasterizeStatsShareCard(). */
async function rasterizeSvg(svg: string, width: number, height: number, scale: number): Promise<Blob | null> {
  const svgDataUri = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  const img = await loadImage(svgDataUri);
  if (!img) return null;

  const canvas = document.createElement("canvas");
  canvas.width = width * scale;
  canvas.height = height * scale;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

  return new Promise((resolve) => {
    canvas.toBlob((blob) => resolve(blob), "image/png");
  });
}

export async function rasterizeShareCard(options: ShareCardOptions, scale = 2): Promise<Blob | null> {
  const exposeFontDataUri = options.exposeFontDataUri ?? (await getExposeFontDataUri());
  const { svg, width, height } = buildShareCardSvg({ ...options, exposeFontDataUri });
  return rasterizeSvg(svg, width, height, scale);
}

export async function rasterizeStatsShareCard(options: StatsShareCardOptions, scale = 2): Promise<Blob | null> {
  const exposeFontDataUri = options.exposeFontDataUri ?? (await getExposeFontDataUri());
  const { svg, width, height } = buildStatsShareCardSvg({ ...options, exposeFontDataUri });
  return rasterizeSvg(svg, width, height, scale);
}

export function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      resolve(result.slice(result.indexOf(",") + 1));
    };
    reader.onerror = reject;
    reader.readAsDataURL(blob);
  });
}
