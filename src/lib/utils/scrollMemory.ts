// Remembers each scrollable view's scrollTop for the lifetime of the app session,
// so navigating away (a different tab, a detail view, back/forward history) and
// returning restores the offset instead of snapping back to the top. Keyed by a
// caller-chosen string identifying "which view" (e.g. `album-detail:${albumName}`)
// rather than tied to Svelte/component state, since the same key can be revisited
// across full component unmount/remount cycles (tab switches destroy and recreate
// these components via {#if}/{#await} blocks).
const positions = new Map<string, number>();

// How many animation frames to keep re-asserting the restored scrollTop for.
// Views whose rows come from an async IPC fetch (e.g. playlist tracks) often
// still have stale/empty content on the frame right after mount — a single
// rAF isn't enough to outlast that round trip — so we keep retrying for a
// short window instead of just once.
const RESTORE_RETRY_FRAMES = 20;

function attachScrollMemory(node: HTMLElement, key: string): () => void {
  let userScrolled = false;
  let lastRestoredValue: number | null = null;

  const restore = () => {
    if (userScrolled) return;
    node.scrollTop = positions.get(key) ?? 0;
    // Read back the actual (possibly clamped, e.g. if content hasn't grown to
    // its final height yet) value the browser applied, not the raw target —
    // otherwise the resulting 'scroll' event won't match and looks like a
    // real user scroll, permanently disabling further retries below.
    lastRestoredValue = node.scrollTop;
  };

  restore();
  let rafId: number | undefined;
  function tick(frame = 0) {
    restore();
    if (frame + 1 < RESTORE_RETRY_FRAMES) {
      rafId = requestAnimationFrame(() => tick(frame + 1));
    }
  }
  rafId = requestAnimationFrame(() => tick(0));

  // Distinguishes the user actually grabbing the scrollbar/wheel from the
  // 'scroll' events our own programmatic restore() calls above trigger.
  const onScroll = () => {
    if (lastRestoredValue !== null && node.scrollTop === lastRestoredValue) {
      lastRestoredValue = null;
      return;
    }
    userScrolled = true;
    positions.set(key, node.scrollTop);
  };
  node.addEventListener("scroll", onScroll, { passive: true });

  return () => {
    if (rafId !== undefined) cancelAnimationFrame(rafId);
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
