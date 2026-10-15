import { SCRIPT_API_VERSION, type LuminousScriptApi } from "./types";
import { methodPaths, negotiateContract } from "./contract";
import { createWaitController } from "./wait";
import { createNavigationController } from "./navigation";
import { createPlaybackController } from "./playback";
import { createAppearanceController } from "./appearance";
import { createDialogsController } from "./dialogs";
import { createViewController } from "./view";
import { createScreensController } from "./screens";

export * from "./types";
export { registerDialogHostControls, type DialogHostControls } from "./dialogs";
export { registerScreenHook, type ScreenHooks } from "./screens";

/**
 * Builds the Luminous in-app scripting API facade.
 */
export function createScriptingApi(): LuminousScriptApi {
  const wait = createWaitController();
  const navigate = createNavigationController(wait);
  const playback = createPlaybackController(wait);
  const appearance = createAppearanceController(wait);
  const dialogs = createDialogsController(wait);
  const view = createViewController(wait);
  const screens = createScreensController(wait);

  const api: LuminousScriptApi = {
    version: SCRIPT_API_VERSION,
    async connect(requires, capabilities = []) {
      return negotiateContract(requires, capabilities, methodPaths(api));
    },
    navigate,
    playback,
    appearance,
    dialogs,
    view,
    screens,
    wait,
  };
  return api;
}

/**
 * Installs the scripting API onto the global window object in debug/dev builds.
 */
export function installScriptingApi(): LuminousScriptApi | undefined {
  if (typeof window === "undefined") return undefined;

  const api = createScriptingApi();
  window.__LUMINOUS_SCRIPT__ = api;
  return api;
}
