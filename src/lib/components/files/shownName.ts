import { i18n } from "../../stores/i18n.svelte";
import type { MissingTag } from "../../types";

const MISSING_TAG_KEYS: Record<MissingTag, string> = {
  artist: "collection.unknownArtist",
  album: "collection.unknownAlbum",
};

/**
 * A file or folder name as the Files view shows it. A Subsonic folder of
 * songs missing a tag has an empty name, worded here; keep using the name
 * itself to navigate and select.
 */
export function shownName(name: string, missingTag: MissingTag | null | undefined): string {
  return missingTag ? i18n.t(MISSING_TAG_KEYS[missingTag]) : name;
}
