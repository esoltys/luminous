import type { registerScreenHook } from "./screens";

type RegisterScreenHook = typeof registerScreenHook;

/**
 * Registers a screen's scripted-capture entry points (see screens.ts) while the screen
 * is mounted. Call it from an `$effect`, behind a guard at the call site so the bundler
 * drops the whole branch, this helper and the screens module included, from release builds:
 * `$effect(() => { if (!import.meta.env.DEV && import.meta.env.MODE !== "capture") return; return onDevScreenHooks((hook) => [...]); })`.
 * Capture builds (`vite build --mode capture`) keep these hooks, since the scripted captures need them.
 */
export function onDevScreenHooks(
  register: (hook: RegisterScreenHook) => Array<() => void>,
): () => void {
  let unregister: Array<() => void> = [];
  let unmounted = false;
  import("./screens").then(({ registerScreenHook }) => {
    if (!unmounted) unregister = register(registerScreenHook);
  });
  return () => {
    unmounted = true;
    unregister.forEach((off) => off());
  };
}
