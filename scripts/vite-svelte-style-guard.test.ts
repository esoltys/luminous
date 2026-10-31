// @vitest-environment node
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createServer, type ViteDevServer } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { svelteVirtualStyleGuard } from "./vite-svelte-style-guard.ts";

const STYLE_QUERY = "?svelte&type=style&lang.css";

describe("svelteVirtualStyleGuard (#1895)", () => {
  let root: string;
  let server: ViteDevServer;

  beforeAll(async () => {
    // Inside the project (gitignored), so the compiled components can resolve
    // `svelte`. Not under node_modules: vite-plugin-svelte injects those
    // components' CSS into their JS instead of emitting a CSS module.
    const cacheDir = join(import.meta.dirname, "..", ".svelte-kit");
    mkdirSync(cacheDir, { recursive: true });
    root = mkdtempSync(join(cacheDir, "style-guard-"));
    writeFileSync(join(root, "Card.svelte"), `<div class="card">x</div>\n<style>.card { position: absolute; }</style>\n`);
    writeFileSync(join(root, "Plain.svelte"), `<div>x</div>\n`);
    server = await createServer({
      configFile: false,
      root,
      logLevel: "silent",
      server: { middlewareMode: true, ws: false, watch: null },
      plugins: [svelte(), svelteVirtualStyleGuard()],
    });
  });

  afterAll(async () => {
    await server?.close();
    rmSync(root, { recursive: true, force: true });
  });

  it("serves a component's CSS when it is requested before the component is compiled", async () => {
    const css = await server.environments.client.transformRequest(`/Card.svelte${STYLE_QUERY}`);
    // The compiled, scoped rule (a raw read of the .svelte file has no hash).
    expect(css?.code).toMatch(/\.card\.svelte-\w+ \{ position: absolute; \}/);

    // The browser's import of the component afterwards still gets the same module.
    const js = await server.environments.client.transformRequest("/Card.svelte");
    expect(js?.code).toContain(`Card.svelte${STYLE_QUERY}`);
  });

  it("serves empty CSS, not the raw .svelte file, for a component without a <style>", async () => {
    const css = await server.environments.client.transformRequest(`/Plain.svelte${STYLE_QUERY}`);
    expect(css?.code).toContain('const __vite__css = ""');
  });
});
