import { defineConfig } from "vitest/config";
import viteConfig from "./vite.config.js";

export default defineConfig(async (env) => {
  const base = await viteConfig(env);
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
