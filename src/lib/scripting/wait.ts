import { tick } from "svelte";
import { listen } from "@tauri-apps/api/event";
import type { ScriptWaitApi } from "./types";

/**
 * Creates the event and state synchronization wait controller.
 */
export function createWaitController(): ScriptWaitApi {
  return {
    async forEvent<T = unknown>(
      eventName: string,
      predicate?: (payload: T) => boolean,
      timeoutMs = 10000
    ): Promise<T> {
      return new Promise<T>((resolve, reject) => {
        let unlistenFn: (() => void) | null = null;
        let settled = false;

        const timer = setTimeout(() => {
          if (settled) return;
          settled = true;
          if (unlistenFn) unlistenFn();
          reject(new Error(`Timed out waiting for event "${eventName}" after ${timeoutMs}ms`));
        }, timeoutMs);

        listen<T>(eventName, (event) => {
          if (settled) return;
          try {
            if (!predicate || predicate(event.payload)) {
              settled = true;
              clearTimeout(timer);
              if (unlistenFn) unlistenFn();
              resolve(event.payload);
            }
          } catch (err) {
            settled = true;
            clearTimeout(timer);
            if (unlistenFn) unlistenFn();
            reject(err);
          }
        })
          .then((unlisten) => {
            if (settled) {
              unlisten();
            } else {
              unlistenFn = unlisten;
            }
          })
          .catch((err) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            reject(err);
          });
      });
    },

    async forState(predicate: () => boolean, timeoutMs = 10000): Promise<void> {
      if (predicate()) return;

      const startTime = Date.now();
      return new Promise<void>((resolve, reject) => {
        const check = () => {
          try {
            if (predicate()) {
              resolve();
              return;
            }
          } catch (err) {
            reject(err);
            return;
          }

          if (Date.now() - startTime >= timeoutMs) {
            reject(new Error(`Timed out waiting for state condition after ${timeoutMs}ms`));
            return;
          }

          setTimeout(check, 25);
        };

        check();
      });
    },

    async settled(): Promise<void> {
      await tick();
      await new Promise<void>((resolve) => {
        if (typeof requestAnimationFrame === "function") {
          requestAnimationFrame(() => resolve());
        } else {
          setTimeout(resolve, 16);
        }
      });
      await tick();
    },
  };
}
