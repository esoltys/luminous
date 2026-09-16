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
  // Without a track list the text block is just three short lines, so the
  // cover can claim a lot more of the frame than when it has to share space
  // with a multi-column list — size each variant for what it's actually
  // sitting next to rather than one flat ratio for both.
  const willShowTrackList = !!(options.includeTrackList && options.tracks && options.tracks.length > 0);
  const coverSize = Math.round(
    isPortrait
      ? width * (willShowTrackList ? 0.56 : 0.72) * contentScale
      : Math.min(width, height) * (willShowTrackList ? 0.46 : 0.6)
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
        ${
          options.coverDataUri
            ? `<img src="${options.coverDataUri}" style="width:${coverSize}px;height:${coverSize}px;object-fit:cover;border-radius:${Math.round(coverSize * 0.06)}px;box-shadow:0 20px 50px rgba(0,0,0,0.4);flex-shrink:0;" />`
            : ""
        }
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

export async function rasterizeShareCard(options: ShareCardOptions, scale = 2): Promise<Blob | null> {
  const exposeFontDataUri = options.exposeFontDataUri ?? (await getExposeFontDataUri());
  const { svg, width, height } = buildShareCardSvg({ ...options, exposeFontDataUri });
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
