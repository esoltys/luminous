import type { Scene } from "./types";
import { collectionScenes } from "./collection";

/** Every scene, in capture order. */
export const scenes: Scene[] = [...collectionScenes];
