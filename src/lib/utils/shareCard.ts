// Social share card builder (#97). The card is composed as a single SVG
// string — the same layered-ellipse gradient used by the immersive view for
// the background, with an <foreignObject> overlay for the actual HTML/CSS
// content (cover art, title, metadata, track list) — then rasterized to a
// PNG by loading that SVG into an <img> and drawing it onto a canvas. This
// avoids pulling in a DOM-to-image dependency: the whole card only ever
// exists as markup we generate, never a live component tree we'd need to
// snapshot.

import { generateEllipseGradientSvg } from "./ellipseGradient";

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
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** How many track rows fit before "+N more" overflow, tuned per aspect ratio's usable height. */
function maxVisibleTracks(dims: { width: number; height: number }): number {
  const isPortrait = dims.height > dims.width;
  const isSquareish = Math.abs(dims.width - dims.height) < dims.width * 0.15;
  if (isPortrait) return 12;
  if (isSquareish) return 6;
  return 5;
}

export function buildShareCardSvg(options: ShareCardOptions): { svg: string; width: number; height: number } {
  const dims = SHARE_ASPECT_RATIOS.find((r) => r.id === options.aspectRatio) ?? SHARE_ASPECT_RATIOS[0];
  const { width, height } = dims;
  const isDark = options.theme === "dark";
  const textPrimary = isDark ? "#0b0c0f" : "#f5f6f8";
  const textSecondary = isDark ? "rgba(11,12,15,0.72)" : "rgba(245,246,248,0.78)";
  const scrimFrom = isDark ? "rgba(255,255,255,0)" : "rgba(0,0,0,0)";
  const scrimTo = isDark ? "rgba(255,255,255,0.55)" : "rgba(0,0,0,0.55)";
  const cardPad = Math.round(width * 0.06);
  // Portrait/square frames stack cover-then-text centered in the middle of
  // the canvas (a bigger cover, since there's little horizontal room);
  // landscape frames keep a side-by-side row so the wide aspect isn't mostly
  // empty gradient either side of a narrow text column.
  const isPortrait = height >= width;
  const coverSize = Math.round(isPortrait ? width * 0.56 : Math.min(width, height) * 0.46);

  const background = generateEllipseGradientSvg({
    width,
    height,
    colors: options.backgroundColors,
    seed: options.seed,
  });
  // Strip the outer <svg ...> wrapper so it can be inlined as this card's own background layer.
  const backgroundInner = background.replace(/^<svg[^>]*>/, "").replace(/<\/svg>$/, "");

  const trackRows: string[] = [];
  if (options.includeTrackList && options.tracks && options.tracks.length > 0) {
    const limit = maxVisibleTracks(dims);
    const visible = options.tracks.slice(0, limit);
    const overflow = options.tracks.length - visible.length;
    for (const track of visible) {
      trackRows.push(
        `<div style="display:flex;gap:10px;align-items:baseline;padding:4px 0;font-size:${Math.round(width * 0.017)}px;color:${textSecondary};">` +
          (track.number != null
            ? `<span style="min-width:1.8em;text-align:right;opacity:0.7;">${track.number}</span>`
            : "") +
          `<span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(track.title)}</span>` +
        `</div>`
      );
    }
    if (overflow > 0) {
      trackRows.push(
        `<div style="padding:4px 0;font-size:${Math.round(width * 0.017)}px;color:${textSecondary};opacity:0.7;">+${overflow} more</div>`
      );
    }
  }

  const showTrackList = trackRows.length > 0;
  const textAlign = isPortrait ? "center" : "left";
  const groupDirection = isPortrait ? "column" : "row";
  const textBlockMaxWidth = isPortrait ? Math.round(width * 0.82) : undefined;

  const contentHtml = `
    <div xmlns="http://www.w3.org/1999/xhtml" style="position:relative;width:100%;height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;padding:${cardPad}px;box-sizing:border-box;font-family:'Inter','Segoe UI',system-ui,sans-serif;">
      <div style="display:flex;flex-direction:${groupDirection};align-items:center;gap:${Math.round(width * 0.035)}px;max-width:100%;">
        ${
          options.coverDataUri
            ? `<img src="${options.coverDataUri}" style="width:${coverSize}px;height:${coverSize}px;object-fit:cover;border-radius:${Math.round(coverSize * 0.06)}px;box-shadow:0 20px 50px rgba(0,0,0,0.4);flex-shrink:0;" />`
            : ""
        }
        <div style="min-width:0;${isPortrait ? "" : "flex:1;"}display:flex;flex-direction:column;gap:2px;align-items:${isPortrait ? "center" : "flex-start"};text-align:${textAlign};${textBlockMaxWidth ? `max-width:${textBlockMaxWidth}px;` : ""}">
          <div style="font-size:${Math.round(width * 0.046)}px;font-weight:800;color:${textPrimary};line-height:1.14;overflow:hidden;text-overflow:ellipsis;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;">${escapeHtml(options.title)}</div>
          <div style="font-size:${Math.round(width * 0.026)}px;font-weight:600;color:${textSecondary};margin-top:6px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:100%;">${escapeHtml(options.subtitle)}</div>
          <div style="font-size:${Math.round(width * 0.019)}px;color:${textSecondary};margin-top:6px;">${escapeHtml(options.metadataLine)}</div>
          ${showTrackList ? `<div style="margin-top:${Math.round(width * 0.025)}px;display:flex;flex-direction:column;text-align:left;width:100%;">${trackRows.join("")}</div>` : ""}
        </div>
      </div>
      <div style="position:absolute;left:${cardPad}px;bottom:${cardPad}px;font-size:${Math.round(width * 0.014)}px;font-weight:700;letter-spacing:0.04em;color:${textSecondary};opacity:0.8;">LUMINOUS</div>
    </div>
  `;

  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">` +
      `<g>${backgroundInner}</g>` +
      `<defs><linearGradient id="scrim" x1="0" y1="0" x2="0" y2="1">` +
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

export async function rasterizeShareCard(options: ShareCardOptions, scale = 2): Promise<Blob | null> {
  const { svg, width, height } = buildShareCardSvg(options);
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
