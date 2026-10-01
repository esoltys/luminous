import type { PlayState } from "../types";
import { i18n, formatNumber } from "../stores/i18n.svelte";

export { formatNumber };

export function formatDuration(ns: number | undefined): string {
  if (!ns) return "0:00";
  const sec = Math.floor(ns / 1_000_000_000);
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s < 10 ? "0" : ""}${s}`;
}

export function formatDate(timestamp?: number): string {
  if (!timestamp) return "—";
  return new Date(timestamp * 1000).toLocaleDateString();
}

export function formatFileSize(bytes?: number): string {
  if (!bytes) return "—";
  if (bytes >= 1073741824) {
    return `${formatNumber(bytes / 1073741824, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} GB`;
  }
  return `${formatNumber(bytes / 1048576, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} MB`;
}

export function formatSampleRate(hz?: number): string {
  if (!hz) return "—";
  return `${formatNumber(hz / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} kHz`;
}

export function formatBitDepth(bits?: number): string {
  if (!bits) return "—";
  return `${bits}-bit`;
}

export function formatChannels(ch?: number): string {
  if (!ch) return "—";
  if (ch === 1) return "Mono";
  if (ch === 2) return "Stereo";
  return `${ch} ch`;
}

export function toTitleCase(str: string): string {
  if (!str) return "";
  return str.replace(/\b\w+/g, (txt) => txt.charAt(0).toUpperCase() + txt.slice(1).toLowerCase());
}

export function formatWindowTitle(
  song?: { title?: string | null; artist?: string | null } | null,
  state?: PlayState,
  appName = "Luminous"
): string {
  if (state !== "playing" || !song) {
    return appName;
  }

  const rawTitle = song.title?.trim();
  const rawArtist = song.artist?.trim();

  const title = rawTitle || i18n.t("collection.unknownSong", {}, "Unknown Song");
  if (rawArtist) {
    return `${title} - ${rawArtist} - ${appName}`;
  }
  return `${title} - ${appName}`;
}

