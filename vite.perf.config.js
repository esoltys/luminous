import { defineConfig } from "vitest/config";
import viteConfig from "./vite.config.js";

export default defineConfig(async (env) => {
  const baseFn = typeof viteConfig === "function" ? viteConfig : () => viteConfig;
  const base = await baseFn(env);
  return {
    ...base,
    resolve: {
      ...base.resolve,
      conditions: ["browser"]
    },
    test: {
      ...base.test,
      include: ["src/**/*.benchmark.ts"],
      environment: "jsdom",
      testTransformMode: {
        web: ["**/*.svelte"]
      }
    }
  };
});
