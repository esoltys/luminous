import { invoke } from "@tauri-apps/api/core";

/**
 * Shared, reference-counted on/off switch for one backend visualizer feed. Every consumer
 * calls `acquire` while it shows the feed's data and `release` when it stops; the backend
 * feed runs while at least one consumer holds it.
 */
export interface FeedToggle {
  acquire(): void;
  release(): void;
}

// Each backend feed is a single global on/off flag shared by every consumer of its events.
// Consumers toggling it directly from their own mount/unmount is unsafe: it isn't
// reference-counted, so one consumer's disable-on-unmount can silently kill the feed for
// another that's still mounted. Worse, a Svelte branch switch (e.g. crossing the
// playbar-only height breakpoint, or toggling Miniplayer) can destroy every consumer and
// recreate fresh ones within the same tick: even with reference counting the count dips
// through zero, and the resulting disable/enable pair of async IPC calls has no guaranteed
// resolve order, so a stale disable can land after the fresh enable and leave the backend
// stuck off.
//
// Debouncing the disable absorbs exactly that destroy-then-immediately-recreate pattern: a
// re-acquire within the window cancels the pending disable outright, so only a feed that's
// genuinely going away for a while is disabled.
const DISABLE_DEBOUNCE_MS = 250;

/** A toggle for the feed the backend switches with `command({ enabled })`. */
export function createFeedToggle(command: string): FeedToggle {
  let refCount = 0;
  let disableTimer: ReturnType<typeof setTimeout> | null = null;

  return {
    acquire() {
      refCount++;
      if (disableTimer) {
        clearTimeout(disableTimer);
        disableTimer = null;
      }
      if (refCount === 1) {
        void invoke(command, { enabled: true }).catch((e) =>
          console.error(`Failed to enable feed (${command}):`, e),
        );
      }
    },
    release() {
      refCount = Math.max(0, refCount - 1);
      if (refCount === 0) {
        disableTimer = setTimeout(() => {
          disableTimer = null;
          void invoke(command, { enabled: false }).catch((e) =>
            console.error(`Failed to disable feed (${command}):`, e),
          );
        }, DISABLE_DEBOUNCE_MS);
      }
    },
  };
}
