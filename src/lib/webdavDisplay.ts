/** Combines a WebDAV server's base URL and remote folder path into one
 * display string, e.g. "http://127.0.0.1:8080" + "/Music/BandCamp/" →
 * "http://127.0.0.1:8080/Music/BandCamp/". */
export function combineWebdavPath(url: string, remotePath: string): string {
  const base = url.trim().replace(/\/+$/, "");
  const path = remotePath.trim();
  if (!path || path === "/") return base || path;
  return `${base}/${path.replace(/^\/+/, "")}`;
}

function safeDecode(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

export interface WebDavMatchResult {
  matches: boolean;
  prefixLength: number;
  base: string;
}

/**
 * Checks whether a song's URL or file path belongs to a WebDAV server endpoint
 * defined by `serverUrl` and `remotePath`. Handles credentials in the song URL,
 * percent-encoding differences (e.g. `%20` vs space), case insensitivity,
 * and path boundary separators.
 */
export function matchWebDavPath(
  songPath: string | null | undefined,
  serverUrl: string | null | undefined,
  remotePath: string | null | undefined
): WebDavMatchResult {
  const safeServerUrl = serverUrl?.trim() ?? "";
  const safeRemotePath = remotePath?.trim() ?? "/";
  const base = combineWebdavPath(safeServerUrl, safeRemotePath);

  if (!songPath || !safeServerUrl) {
    return { matches: false, prefixLength: 0, base };
  }

  const credentialFree = songPath.replace(/^(https?:\/\/)[^@/]*@/i, "$1");

  try {
    const songUrl = new URL(credentialFree);
    const serverBaseUrl = new URL(base);

    if (
      songUrl.protocol.toLowerCase() !== serverBaseUrl.protocol.toLowerCase() ||
      songUrl.host.toLowerCase() !== serverBaseUrl.host.toLowerCase()
    ) {
      return { matches: false, prefixLength: 0, base };
    }

    const songNorm = safeDecode(songUrl.pathname).toLowerCase();
    const serverNorm = safeDecode(serverBaseUrl.pathname).toLowerCase().replace(/\/+$/, "");

    if (!serverNorm || serverNorm === "/") {
      return { matches: true, prefixLength: 1, base };
    }

    if (songNorm === serverNorm || songNorm.startsWith(serverNorm + "/")) {
      return { matches: true, prefixLength: serverNorm.length, base };
    }

    return { matches: false, prefixLength: 0, base };
  } catch {
    // Fallback if URL parsing fails (e.g. mock or non-standard paths)
    const songNorm = safeDecode(credentialFree).toLowerCase();
    const baseNorm = safeDecode(base).toLowerCase().replace(/\/+$/, "");

    if (!baseNorm || baseNorm === "/") {
      return { matches: true, prefixLength: 1, base };
    }

    if (songNorm === baseNorm || songNorm.startsWith(baseNorm + "/")) {
      return { matches: true, prefixLength: baseNorm.length, base };
    }

    return { matches: false, prefixLength: 0, base };
  }
}
