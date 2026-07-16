import type { Plugin } from "vite";
import { compileMockScript } from "./compile-mock-script";
import { loadMockConfig, loadMockLibrary, resolveFeatured } from "./mock-library";
import { existsSync, readFileSync } from "fs";
import { join, dirname } from "path";

export function tauriIpcMockPlugin(): Plugin {
  return {
    name: "luminous-tauri-ipc-mock",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const url = req.url || "";

        if (url.startsWith("/covers/")) {
          try {
            const config = loadMockConfig();
            const dbPath = config.dbPath || "";
            const coversDir = dbPath ? join(dirname(dbPath), "covers") : "";
            if (!coversDir) {
              res.statusCode = 404;
              res.end("No dbPath configured");
              return;
            }
            const filename = decodeURIComponent(url.slice(8));
            const baseName = filename.replace(/\.(jpg|png|jpeg)$/i, "");
            
            let filePath = join(coversDir, filename);
            if (!existsSync(filePath)) {
              const jpgPath = join(coversDir, `${baseName}.jpg`);
              const pngPath = join(coversDir, `${baseName}.png`);
              if (existsSync(jpgPath)) {
                filePath = jpgPath;
              } else if (existsSync(pngPath)) {
                filePath = pngPath;
              }
            }

            if (existsSync(filePath)) {
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
