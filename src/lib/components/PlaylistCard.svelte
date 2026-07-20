<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListMusic } from "lucide-svelte";
  import type { Playlist, PlaylistItem } from "../types";
  import { getArtistGradient } from "../utils/artist";
  import { i18n } from "../stores/i18n.svelte";
  import CoverStack from "./CoverStack.svelte";

  let { playlist, onClick }: { playlist: Playlist; onClick: () => void } = $props();

  let tracks = $state<PlaylistItem[]>([]);

  $effect(() => {
    const id = playlist.id;
    invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: id })
      .then((res) => {
        if (playlist.id === id) {
          tracks = res;
        }
      })
      .catch((err) => {
        console.error("Failed to load playlist tracks for card:", err);
      });
  });

  let topAlbums = $derived.by(() => {
    const seen = new Set<string>();
    const list: Array<{ songId?: number; artEmbedded?: boolean; artAutomatic?: string | null; artManual?: string | null }> = [];
    for (const item of tracks) {
      if (!item.song) continue;
      const s = item.song;
      const key = s.art_manual || s.art_automatic || (s.art_embedded ? `embed-${s.id}` : null);
      if (key && !seen.has(key)) {
        seen.add(key);
        list.push({
          songId: s.id,
          artEmbedded: s.art_embedded,
          artAutomatic: s.art_automatic,
          artManual: s.art_manual,
        });
        if (list.length >= 6) break;
      }
    }
    return list;
  });
</script>

<button
  onclick={onClick}
  class="w-44 shrink-0 bg-brand-sidebar border border-brand-border/60 rounded-xl p-3 flex flex-col text-left hover:border-brand-accent/40 transition-all duration-200 cursor-pointer group"
>
  <div class="aspect-square w-full rounded-lg mb-2.5 overflow-hidden border border-brand-border/60 bg-brand-main relative flex items-center justify-center">
    {#if topAlbums.length > 0}
      <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-24 h-24" />
    {:else}
      <div class="w-full h-full bg-gradient-to-br {getArtistGradient(playlist.name)} flex items-center justify-center">
        <ListMusic class="w-10 h-10 text-white/80" />
      </div>
    {/if}
  </div>

  <span class="font-semibold text-xs text-brand-text-primary group-hover:text-brand-accent-text transition-colors truncate w-full">
    {playlist.name}
  </span>
  <span class="text-[10px] text-brand-text-secondary/50 mt-0.5">
    {playlist.track_count === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: playlist.track_count })}
  </span>
</button>
