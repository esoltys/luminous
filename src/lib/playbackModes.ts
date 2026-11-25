import {
  DiscIcon as DiscAlbum,
  MicrophoneStageIcon as Mic2,
  PlaylistIcon as ListMusic,
  MusicNotesIcon as Music,
} from "phosphor-svelte";
import { i18n } from "./stores/i18n.svelte";
import type { RepeatMode, ShuffleMode } from "./types";

type ModeIcon = typeof DiscAlbum;

interface ModeMeta {
  labelKey: string;
  descriptionKey: string;
  icon: ModeIcon | null;
}

/** Ordered cycle of shuffle modes exposed in the transport controls. */
export const SHUFFLE_MODES = [
  "off",
  "all",
  "inside_album",
  "albums",
  "artists",
] as const satisfies readonly ShuffleMode[];

/**
 * Ordered cycle of repeat modes exposed in the transport controls.
 * `"intro"` exists in `RepeatMode` for backend parity, but is not part of the UI cycle.
 */
export const REPEAT_MODES = [
  "off",
  "track",
  "album",
  "playlist",
] as const satisfies readonly RepeatMode[];

export type CycleableRepeatMode = (typeof REPEAT_MODES)[number];

// A mode-type icon paired alongside the Shuffle/Repeat transport icon to
// disambiguate modes that would otherwise share the same base icon (e.g.
// "Shuffle Albums" and "Shuffle Inside Album" both pair with DiscAlbum) —
// rendered full-size next to the icon rather than as a tiny overlay badge,
// so it stays legible. Reuses the same Phosphor icons as the rest of the app.
const SHUFFLE_MODE_META: Record<ShuffleMode, ModeMeta> = {
  off: {
    labelKey: "playerBar.shuffleOff",
    descriptionKey: "playerBar.shuffleOffDesc",
    icon: null,
  },
  all: {
    labelKey: "playerBar.shuffleAll",
    descriptionKey: "playerBar.shuffleAllDesc",
    icon: null,
  },
  inside_album: {
    labelKey: "playerBar.shuffleInsideAlbum",
    descriptionKey: "playerBar.shuffleInsideAlbumDesc",
    icon: DiscAlbum,
  },
  albums: {
    labelKey: "playerBar.shuffleAlbums",
    descriptionKey: "playerBar.shuffleAlbumsDesc",
    icon: DiscAlbum,
  },
  artists: {
    labelKey: "playerBar.shuffleArtists",
    descriptionKey: "playerBar.shuffleArtistsDesc",
    icon: Mic2,
  },
};

const REPEAT_MODE_META: Record<CycleableRepeatMode, ModeMeta> = {
  off: {
    labelKey: "playerBar.repeatOff",
    descriptionKey: "playerBar.repeatOffDesc",
    icon: null,
  },
  track: {
    labelKey: "playerBar.repeatSong",
    descriptionKey: "playerBar.repeatSongDesc",
    icon: Music,
  },
  album: {
    labelKey: "playerBar.repeatAlbum",
    descriptionKey: "playerBar.repeatAlbumDesc",
    icon: DiscAlbum,
  },
  playlist: {
    labelKey: "playerBar.repeatPlaylist",
    descriptionKey: "playerBar.repeatPlaylistDesc",
    icon: ListMusic,
  },
};

function isCycleableRepeatMode(mode: RepeatMode): mode is CycleableRepeatMode {
  return mode in REPEAT_MODE_META;
}

function cycleNext<T>(modes: readonly T[], current: unknown): T {
  const idx = modes.indexOf(current as T);
  return modes[(idx + 1) % modes.length];
}

/** Returns the next shuffle mode in `SHUFFLE_MODES`, wrapping to `"off"`. */
export function nextShuffleMode(current: ShuffleMode): ShuffleMode {
  return cycleNext(SHUFFLE_MODES, current);
}

/** Returns the next repeat mode in `REPEAT_MODES`, wrapping unlisted modes (e.g. `"intro"`) to `"off"`. */
export function nextRepeatMode(current: RepeatMode): CycleableRepeatMode {
  return cycleNext(REPEAT_MODES, current);
}

export function shuffleModeLabel(mode: ShuffleMode): string {
  return i18n.t(SHUFFLE_MODE_META[mode].labelKey);
}

// Mirrors the mode descriptions from the user guide so the tooltip explains
// what the mode does, not just its name.
export function shuffleModeDescription(mode: ShuffleMode): string {
  return i18n.t(SHUFFLE_MODE_META[mode].descriptionKey);
}

export function shuffleTypeIcon(mode: ShuffleMode): ModeIcon | null {
  return SHUFFLE_MODE_META[mode].icon;
}

/** Full tooltip string combining the "Shuffle" prefix, mode label, and description. */
export function shuffleModeTooltip(mode: ShuffleMode): string {
  return `${i18n.t("playerBar.shuffle")}: ${shuffleModeLabel(mode)} — ${shuffleModeDescription(mode)}`;
}

export function repeatModeLabel(mode: RepeatMode): string {
  if (!isCycleableRepeatMode(mode)) return mode;
  return i18n.t(REPEAT_MODE_META[mode].labelKey);
}

export function repeatModeDescription(mode: RepeatMode): string {
  if (!isCycleableRepeatMode(mode)) return "";
  return i18n.t(REPEAT_MODE_META[mode].descriptionKey);
}

export function repeatTypeIcon(mode: RepeatMode): ModeIcon | null {
  if (!isCycleableRepeatMode(mode)) return null;
  return REPEAT_MODE_META[mode].icon;
}

/** Full tooltip string combining the "Repeat" prefix, mode label, and description. */
export function repeatModeTooltip(mode: RepeatMode): string {
  return `${i18n.t("playerBar.repeat")}: ${repeatModeLabel(mode)} — ${repeatModeDescription(mode)}`;
}
