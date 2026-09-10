<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    PlaylistIcon as ListMusic,
    CalendarIcon as Calendar,
    MusicNotesIcon as Music,
    BroadcastIcon as Radio,
    StackIcon as Layers,
    SparkleIcon as Sparkles
  } from "phosphor-svelte";
  import CardBadge from "./CardBadge.svelte";
  import type { Playlist, PlaylistItem } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import CoverStack from "./CoverStack.svelte";
  import PlaylistCardShell from "./PlaylistCardShell.svelte";
  import PlaylistCoverFrame from "./PlaylistCoverFrame.svelte";
  import { getPlaylistCardTheme } from "../utils/playlistCardTheme";

  import { getPlaylistDisplayName } from "../utils/playlist";

  let {
    playlist,
    onClick,
    widthClass = "w-full",
    oncontextmenu,
    onContextMenu,
  }: {
    playlist: Playlist;
    onClick: () => void;
    widthClass?: string;
    oncontextmenu?: (e: MouseEvent) => void;
    onContextMenu?: (e: MouseEvent) => void;
  } = $props();

  const queueTheme = getPlaylistCardTheme("queue");
  const smartTheme = getPlaylistCardTheme("smart");

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
</script>

{#snippet cover()}
  {#if isQueue}
    <PlaylistCoverFrame gradientClass={queueTheme.gradientClass}>
      <Layers class="w-10 h-10 {queueTheme.iconColorClass}" />
    </PlaylistCoverFrame>
  {:else if topAlbums.length > 0 && autoKind}
    {@const t = getPlaylistCardTheme(autoKind)}
    <PlaylistCoverFrame gradientClass={t.gradientClass}>
      <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
    </PlaylistCoverFrame>
  {:else if topAlbums.length > 0}
    <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
  {:else if autoKind}
    {@const t = getPlaylistCardTheme(autoKind)}
    <PlaylistCoverFrame gradientClass={t.gradientClass}>
      {#if autoKind === "decade"}
        <Calendar class="w-10 h-10 {t.iconColorClass}" />
      {:else if autoKind === "genre"}
        <Music class="w-10 h-10 {t.iconColorClass}" />
      {:else}
        <Sparkles class="w-10 h-10 {t.iconColorClass}" />
      {/if}
    </PlaylistCoverFrame>
  {:else}
    <!-- Custom (user-made) playlist with no art yet: flat, no gradient —
         gradient/frame treatment is reserved for app-generated playlists
         (see Luminous Playlist Card System). -->
    <ListMusic class="w-10 h-10 text-brand-text-secondary" />
  {/if}

  {#if autoKind === "smart"}
    <CardBadge icon={Sparkles} label={i18n.t("playlists.smartBadgeLabel")} title={i18n.t("playlists.smartRuleBasedTooltip")} colorClass={smartTheme.badgeColorClass} />
  {/if}
{/snippet}

{#snippet footer()}
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
{/snippet}

<PlaylistCardShell
  {widthClass}
  {onClick}
  {oncontextmenu}
  {onContextMenu}
  title={cardTitle}
  {subtitleLabel}
  {updatedLabel}
  trackCount={playlist.track_count}
  {cover}
  {footer}
/>
