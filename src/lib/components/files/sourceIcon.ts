import { CloudIcon, DeviceMobileSpeakerIcon, FolderIcon, HardDrivesIcon, UsbIcon } from "phosphor-svelte";
import type { ConnectedDevice, FileSource } from "../../types";

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

/** The icon for a device: a player over MTP, a USB drive otherwise. */
export function deviceIcon(device: ConnectedDevice) {
  return device.kind === "mtp" ? DeviceMobileSpeakerIcon : UsbIcon;
}
