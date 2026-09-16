/** Combines a WebDAV server's base URL and remote folder path into one
 * display string, e.g. "http://127.0.0.1:8080" + "/Music/BandCamp/" →
 * "http://127.0.0.1:8080/Music/BandCamp/". */
export function combineWebdavPath(url: string, remotePath: string): string {
  const base = url.trim().replace(/\/+$/, "");
  const path = remotePath.trim();
  if (!path || path === "/") return base || path;
  return `${base}/${path.replace(/^\/+/, "")}`;
}
