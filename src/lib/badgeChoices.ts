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

/** Shared icon set for library/source badges (watched folders, WebDAV servers).
 * `label` is an i18n key (resolve with `i18n.t(choice.label)`), not display text. */
export const BADGE_ICON_CHOICES: IconChoice[] = [
  { id: "folder", label: "settings.badgeIconFolder", icon: FolderIcon },
  { id: "hard-drive", label: "settings.badgeIconDrive", icon: HardDriveIcon },
  { id: "cloud", label: "settings.badgeIconCloud", icon: CloudIcon },
  { id: "desktop", label: "settings.badgeIconComputer", icon: DesktopIcon },
  { id: "usb", label: "settings.badgeIconUsb", icon: UsbIcon },
  { id: "house", label: "settings.badgeIconHome", icon: HouseIcon },
  { id: "disc", label: "settings.badgeIconDisc", icon: DiscIcon },
  { id: "music", label: "settings.badgeIconMusic", icon: MusicNotesIcon },
  { id: "archive", label: "settings.badgeIconArchive", icon: ArchiveIcon },
  { id: "broadcast", label: "settings.badgeIconShared", icon: BroadcastIcon },
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

/** Shared badge colour palette (watched folders, WebDAV servers, and other future badges).
 * `label` is an i18n key (resolve with `i18n.t(choice.label)`), not display text. */
export const BADGE_COLOR_CHOICES: ColorChoice[] = [
  { value: null, label: "settings.badgeColorDefault" },
  { value: "#3b82f6", label: "settings.badgeColorBlue" },
  { value: "#8b5cf6", label: "settings.badgeColorPurple" },
  { value: "#ec4899", label: "settings.badgeColorPink" },
  { value: "#ef4444", label: "settings.badgeColorRed" },
  { value: "#f97316", label: "settings.badgeColorOrange" },
  { value: "#eab308", label: "settings.badgeColorYellow" },
  { value: "#10b981", label: "settings.badgeColorEmerald" },
  { value: "#06b6d4", label: "settings.badgeColorCyan" },
  { value: "#6366f1", label: "settings.badgeColorIndigo" },
];
