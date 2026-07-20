<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListMusic, Play, Heart, Clock } from "lucide-svelte";
  import type { PlaylistItem, Song } from "../types";
  import { getArtistGradient } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import CoverStack from "./CoverStack.svelte";

  interface Props {
    label: string;
    kind: "favourites" | "recently_added" | "genre";
    genre?: string;
    /** For kind "genre": the materialized playlist row backing it (refreshed at most every 24h). */
    playlistId?: number;
    /** For kind "genre": when this playlist's songs were last (re)generated. */
    updated?: number;
    trackCount: number;
    onClick: () => void;
    widthClass?: string;
  }

  let { label, kind, genre, playlistId, updated, trackCount, onClick, widthClass = "w-full" }: Props = $props();

  let songs = $state<Song[]>([]);

  $effect(() => {
    const k = kind;
    const g = genre;
    const pid = playlistId;

    const request =
      k === "genre" && pid !== undefined
        ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
            items.filter((item) => !!item.song).map((item) => item.song as Song)
          )
        : k === "favourites"
          ? invoke<Song[]>("get_favourite_songs")
          : k === "recently_added"
            ? invoke<Song[]>("get_recently_added_songs", { limit: 50 })
            : invoke<Song[]>("get_songs_by_genre", { genre: g ?? "", limit: 50 });

    request
      .then((res) => {
        if (kind === k && genre === g && playlistId === pid) {
          songs = res;
        }
      })
      .catch((err) => {
        console.error("Failed to load auto-playlist songs for card:", err);
      });
  });

  // Favourites/Recently Added use a fixed icon cover instead of a CoverStack —
  // they're rebuilt from the whole library on every load, so a coverstack of
  // whichever songs happen to be in them right now reads as arbitrary rather
  // than representative (unlike a genre or a user playlist).
  let topCovers = $derived(kind === "genre" ? songsToCoverStack(songs) : []);

  let updatedLabel = $derived.by(() => {
    if (kind !== "genre" || updated === undefined) return null;
    return new Date(updated * 1000).toLocaleDateString();
  });

  function handlePlayButtonClick(e: MouseEvent) {
    e.stopPropagation();
    if (songs.length > 0) {
      playerStore.playSongs(songs.map((s) => s.id), 0);
    }
    onClick();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-xl p-3 flex flex-col text-left hover:border-brand-accent/40 transition-all duration-200 cursor-pointer group"
>
  <div class="aspect-square w-full rounded-lg mb-2.5 bg-brand-main relative flex items-center justify-center">
    {#if kind === "genre" && topCovers.length > 0}
      <CoverStack covers={topCovers} hoverEffect={true} sizeClass="w-24 h-24" />
    {:else if kind === "favourites"}
      <div class="w-full h-full rounded-lg bg-gradient-to-br {getArtistGradient(label)} flex items-center justify-center overflow-hidden border border-brand-border/60">
        <Heart class="w-10 h-10 text-white/80 fill-current" />
      </div>
    {:else if kind === "recently_added"}
      <div class="w-full h-full rounded-lg bg-gradient-to-br {getArtistGradient(label)} flex items-center justify-center overflow-hidden border border-brand-border/60">
        <Clock class="w-10 h-10 text-white/80" />
      </div>
    {:else}
      <div class="w-full h-full rounded-lg bg-gradient-to-br {getArtistGradient(label)} flex items-center justify-center overflow-hidden border border-brand-border/60">
        <ListMusic class="w-10 h-10 text-white/80" />
      </div>
    {/if}
    <div class="absolute inset-0 rounded-lg bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity z-20">
      <button
        onclick={handlePlayButtonClick}
        class="w-12 h-12 rounded-full bg-brand-accent text-brand-accent-contrast flex items-center justify-center scale-75 group-hover:scale-100 transition-transform cursor-pointer"
        title={i18n.t('playerBar.play')}
      >
        <Play class="w-5 h-5 fill-current ml-0.5" />
      </button>
    </div>
  </div>

  <span class="font-semibold text-xs text-brand-text-primary group-hover:text-brand-accent-text transition-colors truncate w-full">
    {label}
  </span>
  <div class="flex items-center justify-between mt-0.5 text-[10px] text-brand-text-secondary/50">
    <span class="truncate">{updatedLabel ? i18n.t('playlists.updatedOn', { date: updatedLabel }) : ""}</span>
    <span class="shrink-0">{trackCount === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: trackCount })}</span>
  </div>
</div>
