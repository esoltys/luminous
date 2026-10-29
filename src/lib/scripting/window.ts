import { invoke } from "@tauri-apps/api/core";
import type { ScriptWaitApi, ScriptWindowApi, ScriptWindowSizeOptions } from "./types";

/**
 * Creates the window and display scale controller.
 */
export function createWindowController(wait: ScriptWaitApi): ScriptWindowApi {
  return {
    async setSize(options: ScriptWindowSizeOptions): Promise<void> {
      await invoke("set_window_size", {
        width: options.width,
        height: options.height,
        scale: options.scale,
      });
      await wait.settled();
    },
    async getScale(): Promise<number> {
      return await invoke<number>("get_window_scale");
    },
  };
}
