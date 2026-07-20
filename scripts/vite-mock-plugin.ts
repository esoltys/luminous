import type { Plugin } from "vite";
import { compileMockScript } from "./compile-mock-script";
import { EMBEDDED_ART_CACHE_DIR } from "./embedded-art-cache";
import { loadMockConfig, loadMockLibrary, resolveDbPath, resolveFeatured } from "./mock-library";
import { existsSync, readFileSync } from "fs";
import { join, dirname } from "path";

/** Tries `dir/filename`, then `dir/<filename without extension>.{jpg,png}`. */
function findCoverFile(dir: string, filename: string): string | undefined {
  const direct = join(dir, filename);
  if (existsSync(direct)) return direct;

  const baseName = filename.replace(/\.(jpg|jpeg|png)$/i, "");
  for (const ext of ["jpg", "png"]) {
    const candidate = join(dir, `${baseName}.${ext}`);
    if (existsSync(candidate)) return candidate;
  }
  return undefined;
}

export function tauriIpcMockPlugin(): Plugin {
  return {
    name: "luminous-tauri-ipc-mock",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const url = req.url || "";

        if (url.startsWith("/covers/")) {
          try {
            const filename = decodeURIComponent(url.slice("/covers/".length));

            // Check the mock-only embedded-art extraction cache first (see
            // embedded-art-cache.ts — it has no real-app equivalent), then
            // fall back to the real app's covers/ directory alongside the
            // resolved db (explicit dbPath or the auto-detected default —
            // *not* the raw config value, which is empty whenever dbPath is
            // left unset for auto-detection).
            const dbPath = resolveDbPath(loadMockConfig());
            const coversDir = dbPath ? join(dirname(dbPath), "covers") : undefined;
            const filePath =
              findCoverFile(EMBEDDED_ART_CACHE_DIR, filename) ?? (coversDir ? findCoverFile(coversDir, filename) : undefined);

            if (filePath) {
              const mime = filePath.endsWith(".png") ? "image/png" : "image/jpeg";
              res.setHeader("Content-Type", mime);
              res.end(readFileSync(filePath));
            } else {
              res.statusCode = 404;
              res.end("Not found");
            }
          } catch (err) {
            next(err as Error);
          }
          return;
        }

        if (url.startsWith("/local-art/")) {
          try {
            const localPath = decodeURIComponent(url.slice(11));
            if (existsSync(localPath)) {
              const mime = localPath.endsWith(".png") ? "image/png" : "image/jpeg";
              res.setHeader("Content-Type", mime);
              res.end(readFileSync(localPath));
            } else {
              res.statusCode = 404;
              res.end("Not found");
            }
          } catch (err) {
            next(err as Error);
          }
          return;
        }

        if (url !== "/tauri-ipc-mock.js") {
          next();
          return;
        }
        try {
          const config = loadMockConfig();
          const library = await loadMockLibrary(config);
          const featured = resolveFeatured(library, {
            featuredSong: config.default?.featuredSong,
            featuredArtist: config.default?.featuredArtist,
            featuredAlbum: config.default?.featuredAlbum,
          });
          const body = [
            `window.__LUMINOUS_MOCK_LIBRARY__ = ${JSON.stringify(library)};`,
            `window.__LUMINOUS_MOCK_FEATURED__ = ${JSON.stringify(featured)};`,
            `window.__LUMINOUS_MOCK_CONFIG__ = ${JSON.stringify(config)};`,
            compileMockScript(),
          ].join("\n");
          res.setHeader("Content-Type", "application/javascript");
          res.end(body);
        } catch (err) {
          next(err as Error);
        }
      });
    },
  };
}
