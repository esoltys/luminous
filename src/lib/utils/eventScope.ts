import {
  listen,
  type EventCallback,
  type EventName,
  type Options,
  type UnlistenFn,
} from "@tauri-apps/api/event";

/**
 * Minimal interface representing the Vite HMR module context (`import.meta.hot`).
 * Defined inline to avoid tight coupling to Vite's environment types.
 */
export interface HmrContext {
  dispose(callback: () => void): void;
}

/**
 * Manages the lifecycle of Tauri event listeners for a store or component.
 *
 * Stores in Luminous are module singletons. During development, Vite hot-module
 * replacement (HMR) re-executes store modules, creating new singletons while the
 * previous singletons' listeners remain registered in Tauri's internal listener map.
 * This leads to duplicate event handling (e.g. multiple toasts for one event).
 *
 * `EventScope` tracks all listener registration promises and their teardowns.
 * When passed `import.meta.hot`, it automatically hooks into Vite's module disposal
 * lifecycle to unregister all listeners before the replacement module executes.
 *
 * In addition, `EventScope` defines the asynchronous registration race out of
 * existence: if `dispose()` is invoked while a `listen()` promise is still pending,
 * the unlistener is called immediately once the promise resolves.
 */
export class EventScope {
  private unlisteners: UnlistenFn[] = [];
  private disposed = false;

  constructor(hot?: HmrContext) {
    if (hot && typeof hot.dispose === "function") {
      hot.dispose(() => this.dispose());
    }
  }

  /**
   * Registers a Tauri event listener within this scope.
   *
   * The returned promise resolves to the unlistener function, identical to
   * `@tauri-apps/api/event`'s `listen`. If this scope is disposed before or
   * while registration is pending, the unlistener will be triggered as soon
   * as the promise resolves to avoid leaking listeners.
   */
  listen<T>(event: EventName, handler: EventCallback<T>, options?: Options): Promise<UnlistenFn> {
    if (this.disposed) {
      return Promise.resolve(() => {});
    }

    const promise =
      options !== undefined ? listen<T>(event, handler, options) : listen<T>(event, handler);
    promise
      .then((unlisten) => {
        if (this.disposed) {
          unlisten();
        } else {
          this.unlisteners.push(unlisten);
        }
      })
      .catch(() => {
        // Ignored: failures to register (e.g. invalid permissions) require no cleanup.
      });

    return promise;
  }

  /**
   * Creates a nested `EventScope` whose lifecycle is bounded by this parent scope.
   * Disposing the parent automatically disposes the child. The child can also be
   * disposed independently (e.g. to reset a subset of listeners) without affecting
   * the parent, in which case it automatically detaches from the parent.
   */
  createChild(): EventScope {
    const child = new EventScope();
    if (this.disposed) {
      child.dispose();
      return child;
    }

    const teardown = () => child.dispose();
    this.unlisteners.push(teardown);

    const originalChildDispose = child.dispose.bind(child);
    child.dispose = () => {
      originalChildDispose();
      const idx = this.unlisteners.indexOf(teardown);
      if (idx !== -1) {
        this.unlisteners.splice(idx, 1);
      }
    };

    return child;
  }

  /**
   * Disposes all listeners registered in this scope. Safe to call multiple times.
   */
  dispose(): void {
    this.disposed = true;
    for (const unlisten of this.unlisteners.splice(0)) {
      try {
        unlisten();
      } catch (err) {
        console.error("Error during EventScope listener disposal:", err);
      }
    }
  }
}

/**
 * Creates an `EventScope` tied optionally to Vite's Hot Module Replacement lifecycle.
 *
 * @example
 * ```ts
 * class PlayerStore {
 *   private events = createEventScope(import.meta.hot);
 *
 *   private async init() {
 *     await this.events.listen("playback-state", (event) => ...);
 *   }
 *
 *   dispose() {
 *     this.events.dispose();
 *   }
 * }
 * ```
 */
export function createEventScope(hot?: HmrContext): EventScope {
  return new EventScope(hot);
}
