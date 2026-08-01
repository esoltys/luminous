// @ts-nocheck
import { defineConfig } from "vitest/config";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { svelteTesting } from "@testing-library/svelte/vite";
import { tauriIpcMockPlugin } from "./scripts/vite-mock-plugin";

import { execSync } from "child_process";

const host = process.env.TAURI_DEV_HOST;
let commitHash = "";
try {
  commitHash = execSync("git rev-parse --short HEAD").toString().trim();
} catch {
  commitHash = "";
}

/** @type {any} */
function safeTailwindcss() {
  const plugins = tailwindcss();
  const pluginArray = Array.isArray(plugins) ? plugins : [plugins];
  return pluginArray.map((plugin) => {
    if (!plugin.transform) return plugin;
    const origTransform = plugin.transform;
    return {
      ...plugin,
      /**
       * @param {string} code
       * @param {string} id
       * @param {any} [options]
       */
      async transform(code, id, options) {
        if (id.includes('?svelte&type=style') || id.includes('&type=style')) {
          return null;
        }
        if (typeof origTransform === 'function') {
          return origTransform.call(this, code, id, options);
        }
        if (origTransform && typeof origTransform.handler === 'function') {
          return origTransform.handler.call(this, code, id, options);
        }
        return null;
      },
    };
  });
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  define: {
    "import.meta.env.VITE_COMMIT_HASH": JSON.stringify(commitHash),
  },
  plugins: [sveltekit(), safeTailwindcss(), svelteTesting(), tauriIpcMockPlugin()],

  test: {
    globals: true,
    include: ["src/**/*.{test,spec}.{js,ts}"],
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
