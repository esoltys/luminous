// The scene format shared by the runner (scene-runner.ts) and the scene files
// in this folder. A scene is mostly data: where the app should be and what it
// should look like. `run` is only for scenes that need real interaction.
import type { DevtoolsDriver } from "../devtools-driver";
import type { LuminousScriptApi } from "../../src/lib/scripting/types";

export type ColorScheme = "light" | "dark";

/** The scripting API as seen from Node: every call is awaited across CDP. */
export type RemoteApi = RemoteOf<Omit<LuminousScriptApi, "version">>;

type RemoteOf<T> = {
  [K in keyof T]: T[K] extends (...args: infer A) => infer R
    ? (...args: A) => Promise<Awaited<R>>
    : T[K] extends object
      ? RemoteOf<T[K]>
      : never;
};

/** The song, album and artist a scene features; resolved against the real library. */
export interface Featured {
  song?: string;
  artist?: string;
  album?: string;
}

interface SceneView {
  tab: string;
  subTab?: string;
  /** Opens Settings on this section instead of `tab`. */
  settings?: string;
}

export interface SceneLayout {
  sidebarOpen?: boolean;
  rightPanelOpen?: boolean;
  sidebarWidth?: number;
  immersive?: boolean;
  miniplayer?: boolean;
}

export interface SceneContext {
  readonly api: RemoteApi;
  readonly driver: DevtoolsDriver;
  readonly locale: string;
  readonly scheme: ColorScheme;
  readonly featured: Featured;
  /** Looks up a UI string in the locale being captured (falls back like the app does). */
  t(keyPath: string): string;
}

export interface Scene {
  /** Unique id; what `--name` matches. */
  name: string;
  /** Output file name, e.g. "albums.png". */
  file: string;
  /**
   * `fresh` scenes run before the library is added (Welcome, empty library,
   * walkthrough); `library` scenes (the default) run after the scan.
   */
  stage?: "fresh" | "library";
  /** Narrow to these locale tags. Defaults to every shipped locale. */
  locales?: string[];
  /** Narrow to these schemes. Defaults to light and dark. */
  schemes?: ColorScheme[];
  /** Write into this folder instead of the scheme folder (e.g. "dynamic"). */
  outputSubdir?: string;
  view?: SceneView;
  theme?: string;
  layout?: SceneLayout;
  viewport?: { width: number; height: number };
  /** Seconds into the featured song when the scene is captured. */
  position?: number;
  featured?: Featured;
  /** Turn online services (lyrics, artist info) on for this scene. Off by default so enrichment toasts stay out of frames. */
  online?: boolean;
  /** Capture only the element matching this selector. */
  clip?: string;
  /** Extra settle time in ms for scenes with slow-loading content. */
  settleMs?: number;
  /** Interaction between reset and capture, for what the declarative fields can't express. */
  run?(ctx: SceneContext): Promise<void>;
  /** Undoes persistent changes `run` made (a column, the seekbar mode) so later scenes start clean. Runs even when the scene fails. */
  cleanup?(ctx: SceneContext): Promise<void>;
}

export function defineScene(scene: Scene): Scene {
  return scene;
}
