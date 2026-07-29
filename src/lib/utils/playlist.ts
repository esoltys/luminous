import type { Playlist, QueuePopulationMode } from "../types";
import { i18n } from "../stores/i18n.svelte";
import { isSmartPlaylistSpec } from "./filterParser";

export function getPopulationModeSuffix(mode: QueuePopulationMode | string | undefined | null): string {
  switch (mode) {
    case "favourites":
      return i18n.t("playlists.populationModeFavourites");
    case "familiar":
      return i18n.t("playlists.populationModeFamiliar");
    case "discover":
      return i18n.t("playlists.populationModeDiscover");
    case "deep_cuts":
      return i18n.t("playlists.populationModeDeepCuts");
    default:
      return "";
  }
}

export function getPlaylistDisplayName(
  playlist: Playlist | { name: string; population_mode?: QueuePopulationMode; dynamic_enabled?: boolean; dynamic_spec?: string } | undefined | null
): string {
  if (!playlist || !playlist.name) return "";
  const baseName = playlist.name;
  if (!playlist.dynamic_enabled || isSmartPlaylistSpec(playlist.dynamic_spec)) {
    return baseName;
  }
  const suffix = getPopulationModeSuffix(playlist.population_mode);
  return suffix ? `${baseName} ${suffix}` : baseName;
}
