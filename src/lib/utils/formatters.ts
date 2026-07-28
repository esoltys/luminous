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
