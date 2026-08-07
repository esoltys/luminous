<script lang="ts">
  import { ListMusic, Calendar, Music, Layers, Sparkles } from "lucide-svelte";
  import type { Playlist } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { getPlaylistDisplayName } from "../utils/playlist";

  let { playlist, onClick }: { playlist: Playlist; onClick: () => void } = $props();

  let cardTitle = $derived(getPlaylistDisplayName(playlist));

  // System genre auto-playlists never reach this component (they render via
  // AutoPlaylistRowCard instead) — mirrors PlaylistCard's autoKind derivation.
  let autoKind = $derived<"genre" | "decade" | "smart" | null>(
    !playlist.dynamic_enabled
      ? null
      : playlist.dynamic_spec?.startsWith("decade:")
        ? "decade"
        : isSmartPlaylistSpec(playlist.dynamic_spec)
          ? "smart"
          : "genre"
  );

  let isQueue = $derived(!playlist.dynamic_enabled && playlist.name.toLowerCase() === "queue");

  let subtitleLabel = $derived.by(() => {
    if (!playlist.dynamic_enabled) return null;
    if (autoKind === "decade") return i18n.t("playlists.decadeAutoPlaylist");
    if (autoKind === "genre") return i18n.t("playlists.genreAutoPlaylist");
    return i18n.t("playlists.smartRulePlaylistLabel");
  });

  let updatedLabel = $derived(formatRelativeDate(playlist.updated));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 cursor-pointer select-none"
>
  <div
    class="relative shrink-0 w-11 h-11 flex items-center justify-center overflow-hidden border {isQueue
      ? 'bg-[#7C3AED]/15 border-[#7C3AED]/30'
      : autoKind === 'decade'
        ? 'bg-[#38BDF8]/15 border-[#38BDF8]/30'
        : autoKind === 'genre'
          ? 'bg-[#34D399]/15 border-[#34D399]/30'
          : autoKind === 'smart'
            ? 'bg-[#F59E0B]/15 border-[#F59E0B]/30'
            : 'bg-brand-main border-brand-border/60'}"
  >
    {#if isQueue}
      <Layers class="w-5 h-5 text-[#7C3AED]" />
    {:else if autoKind === "decade"}
      <Calendar class="w-5 h-5 text-[#38BDF8]" />
    {:else if autoKind === "genre"}
      <Music class="w-5 h-5 text-[#34D399]" />
    {:else if autoKind === "smart"}
      <Sparkles class="w-5 h-5 text-[#F59E0B]" />
    {:else}
      <ListMusic class="w-5 h-5 text-brand-text-secondary" />
    {/if}
  </div>

  <div class="min-w-0 flex-1">
    <p class="truncate text-sm font-semibold text-brand-text-primary" title={cardTitle}>{cardTitle}</p>
    {#if subtitleLabel}
      <p class="truncate text-xs text-brand-text-secondary font-medium">{subtitleLabel}</p>
    {/if}
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">
      {playlist.track_count === 1 ? i18n.t("playlists.oneSong") : i18n.t("playlists.songsCount", { count: playlist.track_count })}
    </p>
    <p class="text-xs text-brand-text-secondary truncate">{updatedLabel}</p>
  </div>
</div>
