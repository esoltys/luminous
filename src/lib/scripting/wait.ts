import { tick } from "svelte";
import { listen } from "@tauri-apps/api/event";
import { themeStore } from "../stores/theme.svelte";
import { playerStore } from "../stores/player.svelte";
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

    async visualsSettled(timeoutMs = 10000): Promise<void> {
      const startTime = Date.now();
      const remainingTime = () => Math.max(100, timeoutMs - (Date.now() - startTime));

      // 1. Initial microtasks and DOM layout settlement
      await this.settled();

      // 2. Dynamic artwork palette wait
      if (themeStore.activeThemeId === "dynamic-artwork" && playerStore.currentSong) {
        await this.forState(() => !themeStore.isExtractingArtworkColors, remainingTime());
      }

      // 3. Waveform / band seekbar loading wait
      if (playerStore.currentSong) {
        await this.forState(() => {
          if (typeof document === "undefined") return true;
          const loadingEls = document.querySelectorAll('[data-waveform-loading="true"]');
          return loadingEls.length === 0;
        }, remainingTime());
      }

      // 4. Artwork decode wait: all <img> elements
      if (typeof document !== "undefined") {
        const images = Array.from(document.querySelectorAll("img"));
        await Promise.all(
          images.map((img) => {
            if (img.complete) {
              return img.naturalWidth > 0 && typeof img.decode === "function"
                ? img.decode().catch(() => {})
                : Promise.resolve();
            }
            return new Promise<void>((resolve) => {
              let timer: ReturnType<typeof setTimeout> | null = null;
              const finish = () => {
                if (timer) clearTimeout(timer);
                img.removeEventListener("load", finish);
                img.removeEventListener("error", finish);
                if (typeof img.decode === "function") {
                  img.decode().catch(() => {}).finally(() => resolve());
                } else {
                  resolve();
                }
              };
              img.addEventListener("load", finish, { once: true });
              img.addEventListener("error", finish, { once: true });
              timer = setTimeout(finish, Math.min(2000, remainingTime()));
            });
          })
        );
      }

      // 5. Final render and frame paint flush
      await this.settled();
    },
  };
}
