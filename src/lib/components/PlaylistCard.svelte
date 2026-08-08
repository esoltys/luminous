<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListMusic, Calendar, Music, Radio, Layers, Sparkles } from "lucide-svelte";
  import CardBadge from "./CardBadge.svelte";
  import type { Playlist, PlaylistItem } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import CoverStack from "./CoverStack.svelte";

  import { getPlaylistDisplayName } from "../utils/playlist";

  let { playlist, onClick, widthClass = "w-full" }: { playlist: Playlist; onClick: () => void; widthClass?: string } = $props();

  let cardTitle = $derived(getPlaylistDisplayName(playlist));

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

  let topAlbums = $derived(songsToCoverStack(tracks.filter((t) => !!t.song).map((t) => t.song!)));

  // System genre auto-playlists store a bare genre name (no ':') and never
  // reach this component (they render via AutoPlaylistCard instead).
  let autoKind = $derived<"genre" | "decade" | "smart" | null>(
    !playlist.dynamic_enabled
      ? null
      : playlist.dynamic_spec?.startsWith("decade:")
      ? "decade"
      : isSmartPlaylistSpec(playlist.dynamic_spec)
      ? "smart"
      : "genre"
  );

  let subtitleLabel = $derived.by(() => {
    if (!playlist.dynamic_enabled) return null;
    if (autoKind === "decade") return i18n.t("playlists.decadeAutoPlaylist");
    if (autoKind === "genre") return i18n.t("playlists.genreAutoPlaylist");
    return i18n.t("playlists.smartRulePlaylistLabel");
  });

  let isQueue = $derived(playlist.is_queue);
  let isActive = $derived(playlistsStore.effectivePinnedPlaylistId === playlist.id);

  let updatedLabel = $derived(formatRelativeDate(playlist.updated));

  let badgeColorClass = $derived.by(() => {
    switch (autoKind) {
      case "decade": return "bg-[#2563EB] text-white";
      case "genre": return "bg-[#059669] text-white";
      case "smart": return "bg-[#C2410C] text-white";
      default: return "bg-brand-accent text-brand-accent-contrast";
    }
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col text-left hover:border-brand-accent/40 transition-all duration-200 group relative"
>
  <div class="aspect-square w-full mb-3 bg-brand-main relative flex items-center justify-center overflow-hidden">
    {#if isQueue}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#4338CA]/25 to-[#7C3AED]/15 flex items-center justify-center overflow-hidden border border-[#7C3AED]/30 shadow-[0_0_20px_2px_rgba(124,58,237,0.35)]">
        <Layers class="w-10 h-10 text-[#7C3AED]" />
      </div>
    {:else if topAlbums.length > 0 && autoKind}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br {autoKind === 'decade' ? 'from-[#2563EB]/25 to-[#38BDF8]/15 border-[#38BDF8]/30 shadow-[0_0_20px_2px_rgba(56,189,248,0.35)]' : autoKind === 'genre' ? 'from-[#059669]/25 to-[#34D399]/15 border-[#34D399]/30 shadow-[0_0_20px_2px_rgba(52,211,153,0.35)]' : 'from-[#C2410C]/25 to-[#F59E0B]/15 border-[#F59E0B]/30 shadow-[0_0_20px_2px_rgba(245,158,11,0.35)]'} flex items-center justify-center overflow-hidden border">
        <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
      </div>
    {:else if topAlbums.length > 0}
      <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
    {:else if autoKind}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br {autoKind === 'decade' ? 'from-[#2563EB]/25 to-[#38BDF8]/15 border-[#38BDF8]/30 shadow-[0_0_20px_2px_rgba(56,189,248,0.35)]' : autoKind === 'genre' ? 'from-[#059669]/25 to-[#34D399]/15 border-[#34D399]/30 shadow-[0_0_20px_2px_rgba(52,211,153,0.35)]' : 'from-[#C2410C]/25 to-[#F59E0B]/15 border-[#F59E0B]/30 shadow-[0_0_20px_2px_rgba(245,158,11,0.35)]'} flex items-center justify-center overflow-hidden border">
        {#if autoKind === "decade"}
          <Calendar class="w-10 h-10 text-[#38BDF8]" />
        {:else if autoKind === "genre"}
          <Music class="w-10 h-10 text-[#34D399]" />
        {:else}
          <Sparkles class="w-10 h-10 text-[#F59E0B]" />
        {/if}
      </div>
    {:else}
      <!-- Custom (user-made) playlist with no art yet: flat, no gradient —
           gradient/frame treatment is reserved for app-generated playlists
           (see Luminous Playlist Card System). -->
      <ListMusic class="w-10 h-10 text-brand-text-secondary" />
    {/if}

    {#if (autoKind === "decade" || autoKind === "genre") && playlist.auto_play}
      <CardBadge icon={Radio} label={i18n.t('playlists.autoPlayBadgeLabel')} title={i18n.t('playlists.autoPlayBadgeTooltip')} colorClass={badgeColorClass} />
    {:else if autoKind === "smart"}
      <CardBadge icon={Sparkles} label={i18n.t("playlists.smartBadgeLabel")} title={i18n.t("playlists.smartRuleBasedTooltip")} colorClass={badgeColorClass} />
    {/if}
  </div>

  <button
    onclick={(e) => { e.stopPropagation(); onClick(); }}
    class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate w-full"
    title={cardTitle}
  >
    {cardTitle}
  </button>
  {#if subtitleLabel}
    <div class="text-xs text-brand-text-secondary truncate w-full mt-0.5 font-medium">
      {subtitleLabel}
    </div>
  {/if}
  <div class="flex items-center justify-between mt-0.5 text-xs text-brand-text-secondary">
    <span class="truncate">{updatedLabel}</span>
    <span class="shrink-0">{playlist.track_count === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: playlist.track_count })}</span>
  </div>

  {#if !playlist.dynamic_enabled}
    {#if !isActive}
      <button
        onclick={(e) => { e.stopPropagation(); playlistsStore.pinPlaylist(playlist.id); }}
        class="mt-2.5 w-full py-1 px-2.5 text-xs font-semibold rounded-lg bg-brand-main/80 hover:bg-brand-accent hover:text-brand-accent-contrast border border-brand-border/60 text-brand-text-secondary hover:border-transparent transition-all duration-150 flex items-center justify-center gap-1.5 shadow-xs"
        title={i18n.t('playlists.makeActiveBtn')}
      >
        <Radio class="w-3.5 h-3.5 text-brand-accent-text group-hover:text-current" />
        <span>{i18n.t('playlists.makeActiveBtn')}</span>
      </button>
    {:else}
      <div class="mt-2.5 w-full py-1 px-2.5 text-xs font-semibold rounded-lg bg-brand-accent/15 text-brand-accent-text border border-brand-accent/30 flex items-center justify-center gap-1.5 select-none">
        <Radio class="w-3.5 h-3.5 text-brand-accent-text animate-pulse" />
        <span>{i18n.t('playlists.activeBadgeLabel')}</span>
      </div>
    {/if}
  {/if}
</div>
