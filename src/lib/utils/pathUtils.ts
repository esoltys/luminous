/**
 * Utility functions for path and folder manipulation across local filesystem
 * paths (Windows & Unix) and remote URLs (e.g. WebDAV).
 */

/**
 * Returns true if the directory name represents a disc or CD subfolder
 * (e.g. "Disc 1", "disc 02", "CD1", "CD 2", "Disk 1", "CD A").
 */
export function isDiscFolder(folderName: string): boolean {
  if (!folderName) return false;
  const trimmed = folderName.trim();
  // Matches "Disc 1", "CD1", "CD 2", "Disk 01", or with letter "CD A", "Disc - B"
  return (
    /^(?:disc|disk|cd)\s*\d+$/i.test(trimmed) ||
    /^(?:disc|disk|cd)(?:\s+|[\s-_]+)[a-z]$/i.test(trimmed)
  );
}

/**
 * Extracts the containing directory / folder for a given file path or URL.
 * Supports Windows backslashes (`\`), Unix forward slashes (`/`), and URLs.
 */
export function getParentFolder(filePath: string): string {
  if (!filePath) return "";

  // Remote URL handling (e.g. WebDAV streams)
  if (/^https?:\/\//i.test(filePath)) {
    try {
      const url = new URL(filePath);
      const cleanPath = url.pathname.replace(/\/+$/, "");
      const lastSlash = cleanPath.lastIndexOf("/");
      if (lastSlash > 0) {
        url.pathname = cleanPath.slice(0, lastSlash);
        return url.toString();
      } else if (lastSlash === 0) {
        url.pathname = "/";
        return url.toString();
      }
      return filePath;
    } catch {
      const clean = filePath.replace(/\/+$/, "");
      const idx = clean.lastIndexOf("/");
      return idx > 0 ? clean.slice(0, idx) : filePath;
    }
  }

  // Filesystem paths (Windows or Unix)
  const clean = filePath.replace(/[/\\]+$/, "");
  const lastBackslash = clean.lastIndexOf("\\");
  const lastSlash = clean.lastIndexOf("/");
  const lastSep = Math.max(lastBackslash, lastSlash);

  if (lastSep < 0) {
    return "";
  }

  // Windows drive letter root: "C:\file.mp3" -> "C:\"
  const dir = clean.slice(0, lastSep);
  if (/^[a-zA-Z]:$/.test(dir)) {
    return clean.slice(0, lastSep + 1);
  }

  // Unix root: "/file.mp3" -> "/"
  if (lastSep === 0) {
    return clean.charAt(0);
  }

  return dir;
}

/**
 * Returns the folder name (last path component) of a folder path.
 */
export function getFolderName(folderPath: string): string {
  if (!folderPath) return "";
  const clean = folderPath.replace(/[/\\]+$/, "");
  const lastBackslash = clean.lastIndexOf("\\");
  const lastSlash = clean.lastIndexOf("/");
  const lastSep = Math.max(lastBackslash, lastSlash);
  if (lastSep < 0) return clean;
  return clean.slice(lastSep + 1);
}

/**
 * Resolves the album folder path for a collection of song paths.
 * If the songs live inside disc subfolders (e.g. `Album/Disc 1/track.flac` or
 * `Album/CD2/track.flac`), this steps up to the containing album directory.
 */
export function getAlbumFolderPath(paths: string[]): string {
  const validPaths = paths.filter((p) => Boolean(p && p.trim()));
  if (validPaths.length === 0) return "";

  // Extract the containing folder for each song, stripping any disc subfolder
  const folders = validPaths.map((p) => {
    let folder = getParentFolder(p);
    const name = getFolderName(folder);
    if (isDiscFolder(name)) {
      const parentOfDisc = getParentFolder(folder);
      if (parentOfDisc) {
        folder = parentOfDisc;
      }
    }
    return folder;
  });

  // If all songs point to the same folder, return it directly
  const firstFolder = folders[0];
  if (folders.every((f) => f === firstFolder)) {
    return firstFolder;
  }

  // Otherwise, find the common directory prefix among the folders
  const isWindows = firstFolder.includes("\\");
  const sep = isWindows ? "\\" : "/";
  const normalizedFolders = folders.map((f) => f.split(/[/\\]/));

  const commonSegments: string[] = [];
  const minLength = Math.min(...normalizedFolders.map((parts) => parts.length));

  for (let i = 0; i < minLength; i++) {
    const seg = normalizedFolders[0][i];
    const matchesAll = normalizedFolders.every((parts) =>
      isWindows ? parts[i].toLowerCase() === seg.toLowerCase() : parts[i] === seg
    );
    if (matchesAll) {
      commonSegments.push(seg);
    } else {
      break;
    }
  }

  if (commonSegments.length === 0) {
    return firstFolder;
  }

  // Check if root drive e.g. ["C:"]
  if (commonSegments.length === 1 && /^[a-zA-Z]:$/.test(commonSegments[0])) {
    return commonSegments[0] + sep;
  }

  return commonSegments.join(sep);
}
