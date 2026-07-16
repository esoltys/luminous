// Dev-only Vite plugin serving the compiled Tauri IPC mock at
// /tauri-ipc-mock.js, so it can be dropped into a plain browser tab running
// `bun run dev` without a Tauri backend. Compiles scripts/tauri-ipc-mock.ts
// fresh on every request — no build step, no static artifact to go stale.
import type { Plugin } from "vite";
import { compileMockScript } from "./compile-mock-script";
import { loadMockConfig, loadMockLibrary, resolveFeatured } from "./mock-library";

export function tauriIpcMockPlugin(): Plugin {
  return {
    name: "luminous-tauri-ipc-mock",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        if (req.url !== "/tauri-ipc-mock.js") {
          next();
          return;
        }
        try {
          const config = loadMockConfig();
          const library = await loadMockLibrary(config);
          const featured = resolveFeatured(library, config);
          const body = [
            `window.__LUMINOUS_MOCK_LIBRARY__ = ${JSON.stringify(library)};`,
            `window.__LUMINOUS_MOCK_FEATURED__ = ${JSON.stringify(featured)};`,
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
