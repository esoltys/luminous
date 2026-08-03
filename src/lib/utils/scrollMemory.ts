// Remembers each scrollable view's scrollTop for the lifetime of the app session,
// so navigating away (a different tab, a detail view, back/forward history) and
// returning restores the offset instead of snapping back to the top. Keyed by a
// caller-chosen string identifying "which view" (e.g. `album-detail:${albumName}`)
// rather than tied to Svelte/component state, since the same key can be revisited
// across full component unmount/remount cycles (tab switches destroy and recreate
// these components via {#if}/{#await} blocks).
const positions = new Map<string, number>();

function attachScrollMemory(node: HTMLElement, key: string): () => void {
  const restore = () => {
    node.scrollTop = positions.get(key) ?? 0;
  };
  // Restore immediately for content whose layout is already correct, and again
  // next frame for content (e.g. virtualized lists) whose height settles late.
  restore();
  const raf = requestAnimationFrame(restore);
  const onScroll = () => positions.set(key, node.scrollTop);
  node.addEventListener("scroll", onScroll, { passive: true });
  return () => {
    cancelAnimationFrame(raf);
    node.removeEventListener("scroll", onScroll);
  };
}

/**
 * Svelte action: restores `node.scrollTop` from the cache for `key` on mount,
 * and keeps the cache updated as the user scrolls. Pass a falsy key to opt out
 * (e.g. before the view's identity — such as an album name — is known).
 */
export function rememberScroll(node: HTMLElement, key: string | null | undefined) {
  let detach = key ? attachScrollMemory(node, key) : () => {};
  return {
    update(newKey: string | null | undefined) {
      detach();
      detach = newKey ? attachScrollMemory(node, newKey) : () => {};
    },
    destroy() {
      detach();
    },
  };
}

/**
 * Non-action variant for elements not reached via a template ref, e.g. the
 * scrolling viewport a third-party virtual-list component creates internally.
 * Call from an `$effect` once the element exists; use the returned cleanup.
 */
export function watchScrollMemory(node: HTMLElement, key: string): () => void {
  return attachScrollMemory(node, key);
}
