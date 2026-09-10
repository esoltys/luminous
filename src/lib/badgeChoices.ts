import type { Component } from "svelte";
import {
  FolderIcon,
  HardDriveIcon,
  CloudIcon,
  DesktopIcon,
  UsbIcon,
  HouseIcon,
  DiscIcon,
  MusicNotesIcon,
  ArchiveIcon,
  BroadcastIcon,
} from "phosphor-svelte";

export interface IconChoice {
  id: string;
  label: string;
  icon: Component<any>;
}

/** Shared icon set for library/source badges (watched folders, WebDAV servers). */
export const BADGE_ICON_CHOICES: IconChoice[] = [
  { id: "folder", label: "Folder", icon: FolderIcon },
  { id: "hard-drive", label: "Drive", icon: HardDriveIcon },
  { id: "cloud", label: "Cloud / NAS", icon: CloudIcon },
  { id: "desktop", label: "Computer", icon: DesktopIcon },
  { id: "usb", label: "USB", icon: UsbIcon },
  { id: "house", label: "Home", icon: HouseIcon },
  { id: "disc", label: "Disc", icon: DiscIcon },
  { id: "music", label: "Music", icon: MusicNotesIcon },
  { id: "archive", label: "Archive", icon: ArchiveIcon },
  { id: "broadcast", label: "Shared", icon: BroadcastIcon },
];

export interface ColorChoice {
  value: string | null;
  label: string;
  /** CSS color for the swatch background, when it differs from `value`
   * itself — e.g. an `hsl(...)` string for a palette keyed by index rather
   * than by hex (see genrePalette.ts's `getGenreColorChoices`). Falls back
   * to `value`. */
  swatchColor?: string;
}

/** Shared badge colour palette (watched folders, WebDAV servers, and other future badges). */
export const BADGE_COLOR_CHOICES: ColorChoice[] = [
  { value: null, label: "Default" },
  { value: "#3b82f6", label: "Blue" },
  { value: "#8b5cf6", label: "Purple" },
  { value: "#ec4899", label: "Pink" },
  { value: "#ef4444", label: "Red" },
  { value: "#f97316", label: "Orange" },
  { value: "#eab308", label: "Yellow" },
  { value: "#10b981", label: "Emerald" },
  { value: "#06b6d4", label: "Cyan" },
  { value: "#6366f1", label: "Indigo" },
];
