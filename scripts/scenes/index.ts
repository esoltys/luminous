import type { Scene } from "./types";
import { collectionScenes } from "./collection";
import { dynamicScenes } from "./dynamic";
import { firstRunScenes } from "./first-run";
import { playlistScenes } from "./playlists";
import { settingsScenes } from "./settings";
import { windowScenes } from "./windows";

/** Every scene, in capture order. Miniplayer scenes always run last in a pass. */
export const scenes: Scene[] = [
  ...firstRunScenes,
  ...collectionScenes,
  ...playlistScenes,
  ...settingsScenes,
  ...dynamicScenes,
  ...windowScenes,
];
