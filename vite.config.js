// @ts-nocheck
import { defineConfig } from "vitest/config";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { svelteTesting } from "@testing-library/svelte/vite";
import { tauriIpcMockPlugin } from "./scripts/vite-mock-plugin.ts";

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
    if (!plugin || !plugin.transform) return plugin;
    const rawTransform = plugin.transform;
    const transformFn =
      typeof rawTransform === "function" ? rawTransform : rawTransform.handler;
    if (typeof transformFn !== "function") return plugin;

    const wrappedTransform = async function (code, id, options) {
      if (id.includes("?svelte&type=style") || id.includes("&type=style")) {
        return null;
      }
      return transformFn.call(this, code, id, options);
    };

    if (typeof rawTransform === "object") {
      return {
        ...plugin,
        transform: {
          ...rawTransform,
          handler: wrappedTransform,
        },
      };
    }

    return {
      ...plugin,
      transform: wrappedTransform,
    };
  });
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  css: {
    devSourcemap: false,
  },
  build: {
    // Silence Rolldown's `[PLUGIN_TIMINGS]` warnings (Vite 8 defaults to
    // warning when a plugin's transform takes a while during build).
    rolldownOptions: {
      checks: {
        pluginTimings: false,
      },
    },
  },
  define: {
    "import.meta.env.VITE_COMMIT_HASH": JSON.stringify(commitHash),
  },
  plugins: [sveltekit(), safeTailwindcss(), svelteTesting(), tauriIpcMockPlugin()],

  // These are only reachable once specific views actually render (icons,
  // Tauri API shims, the virtualized song list), not from the initial
  // route's static import graph — so Vite's dep scanner misses them at
  // startup and discovers them on first navigation instead, forcing a full
  // client reload mid-session. Declaring them here bundles them upfront so
  // dev (and the take-screenshots harness, which navigates straight into
  // arbitrary views) never hits that reload.
  optimizeDeps: {
    include: [
      "lucide-svelte",
      "@tauri-apps/api/core",
      "@tauri-apps/api/event",
      "@tauri-apps/api/window",
      "@tauri-apps/api/app",
      "@tauri-apps/plugin-dialog",
      "@tauri-apps/plugin-opener",
      "@tauri-apps/plugin-updater",
      "@tauri-apps/plugin-process",
      "svelte-virtual-list-ts",
    ],
  },

  test: {
    globals: true,
    include: ["src/**/*.{test,spec}.{js,ts}"],
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    testTimeout: 15000,
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
    fs: {
      allow: ["..", "../.."],
    },
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri` and `target` build outputs
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },
}));
