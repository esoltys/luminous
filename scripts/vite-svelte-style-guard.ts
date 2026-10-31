// Vite dev-server plugin that keeps a Svelte component's virtual CSS module
// (`?svelte&type=style&lang.css`) from being served empty when it's requested
// before the component itself has been compiled (#1895).
import type { DevEnvironment, Plugin } from "vite";

/**
 * vite-plugin-svelte only reads a component's CSS from its compile result, so
 * a cold request for the CSS module finds nothing. Vite's default file loader
 * would then feed the raw .svelte file into PostCSS, and returning empty CSS
 * isn't safe either: Vite caches it until the .svelte file changes. WebKit hits
 * this on every `tauri dev` launch: its disk cache replays the previous
 * session's subresources for `localhost:1420`, CSS modules included, in
 * parallel with the page, before any component is compiled.
 *
 * So compile the component first, through the same request path the browser's
 * own import of it takes, then let vite-plugin-svelte's loader serve the CSS.
 * Dev server only: a build always compiles a component before its CSS module.
 */
export function svelteVirtualStyleGuard(): Plugin {
  return {
    name: "svelte-virtual-style-guard",
    apply: "serve",
    enforce: "pre",
    async load(id) {
      if (!id.includes(".svelte") || !(id.includes("?svelte&type=style") || id.includes("&type=style"))) {
        return;
      }
      const file = id.split("?", 1)[0];
      const compiledCss = () => this.getModuleInfo(file)?.meta.svelte?.css;
      if (!compiledCss()) {
        // `apply: "serve"`, so this is always the dev server's environment.
        await (this.environment as DevEnvironment).transformRequest(`/@fs/${file.replace(/^\//, "")}`);
      }
      // Compiled CSS: leave it to vite-plugin-svelte. None (a stale request
      // for a component that no longer has a <style>): serve it empty.
      return compiledCss() ? undefined : { code: "", map: null };
    },
  };
}
