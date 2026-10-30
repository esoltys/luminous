import { CloudIcon, DeviceMobileSpeakerIcon, FolderIcon, HardDrivesIcon } from "phosphor-svelte";
import type { FileSource } from "../../types";

/** The icon that stands for a kind of source. */
export function sourceIcon(source: FileSource) {
  switch (source.kind) {
    case "subsonic":
      return HardDrivesIcon;
    case "webdav":
      return CloudIcon;
    case "device":
      return DeviceMobileSpeakerIcon;
    default:
      return FolderIcon;
  }
}
