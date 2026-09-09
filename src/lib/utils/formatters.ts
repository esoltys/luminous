import type { PlayState } from "../types";
import { i18n } from "../stores/i18n.svelte";

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
    return `${(bytes / 1073741824).toFixed(1)} GB`;
  }
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

export function formatSampleRate(hz?: number): string {
  if (!hz) return "—";
  return `${(hz / 1000).toFixed(1)} kHz`;
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
  state?: PlayState
): string {
  if (state !== "playing" || !song) {
    return "Luminous";
  }

  const rawTitle = song.title?.trim();
  const rawArtist = song.artist?.trim();

  const title = rawTitle || i18n.t("collection.unknownSong", {}, "Unknown Song");
  if (rawArtist) {
    return `${title} - ${rawArtist} - Luminous`;
  }
  return `${title} - Luminous`;
}

