<script lang="ts">
  import { Tag as TagIcon, ChevronRight, ChevronDown, ArrowLeft } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { tagsStore } from "../stores/tags.svelte";
  import { prefs, type GenreViewMode } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import EmptyState from "./EmptyState.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import type { Song } from "../types";

  let expandedRoots = $state<Set<string>>(new Set());
  let selectedTag = $state<string | null>(null);
  let drillDownSongs = $state<Song[]>([]);
  let drillDownLoading = $state(false);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  function handleContextMenu(e: MouseEvent, song: Song) {
    e.preventDefault();
    contextMenuState = { x: e.clientX, y: e.clientY, song };
  }

  function toggleExpanded(mainTag: string) {
    const next = new Set(expandedRoots);
    if (next.has(mainTag)) {
      next.delete(mainTag);
    } else {
      next.add(mainTag);
    }
    expandedRoots = next;
  }

  async function openTag(tagName: string) {
    selectedTag = tagName;
    drillDownLoading = true;
    try {
      drillDownSongs = await tagsStore.getSongsByTag(tagName, 500);
    } finally {
      drillDownLoading = false;
    }
  }

  function setViewMode(mode: GenreViewMode) {
    prefs.setGenreViewMode(mode);
    selectedTag = null;
  }

  async function playAllInTag() {
    if (drillDownSongs.length === 0) return;
    await playerStore.playSongs(
      drillDownSongs.map((s) => s.id),
      0,
      undefined,
      { type: "song" },
      selectedTag ?? undefined
    );
  }
</script>

<div class="flex-1 px-6 pt-4 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
  {#if selectedTag !== null}
    <div class="h-9 flex items-center gap-2 mb-3">
      <button
        onclick={() => { selectedTag = null; }}
        class="flex items-center gap-1.5 text-xs font-medium text-brand-text-secondary hover:text-brand-text-primary transition-colors"
      >
        <ArrowLeft class="w-3.5 h-3.5" />
        {i18n.t("common.back", {}, "Back")}
      </button>
      <span class="text-brand-text-secondary/40">/</span>
      <h2 class="text-sm font-bold text-brand-text-primary truncate">{selectedTag}</h2>
    </div>

    {#if drillDownLoading}
      <div class="py-16 text-center text-sm text-brand-text-secondary">{i18n.t("common.loading", {}, "Loading…")}</div>
    {:else}
      {#if drillDownSongs.length > 0}
        <button
          onclick={playAllInTag}
          class="mb-3 text-xs font-semibold text-brand-accent-text hover:text-brand-accent-text-hover transition-colors"
        >
          {i18n.t("songTags.playAll", { count: drillDownSongs.length }, `Play all ${drillDownSongs.length} songs`)}
        </button>
      {/if}
      <div class="flex flex-col gap-2">
        {#each drillDownSongs as song (song.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            role="button"
            tabindex="0"
            onclick={() => playerStore.playSong(song.id)}
            oncontextmenu={(e) => handleContextMenu(e, song)}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); playerStore.playSong(song.id); } }}
            class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
          >
            <CoverArt
              songId={song.id}
              artEmbedded={song.art_embedded}
              artAutomatic={song.art_automatic}
              artManual={song.art_manual}
              sizeClass="w-11 h-11 shrink-0"
            />
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <p class="truncate text-sm font-semibold text-brand-text-primary">{song.title || i18n.t('collection.unknownSong')}</p>
                <span class="shrink-0">
                  <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
                </span>
              </div>
              <p class="truncate text-xs text-brand-text-secondary font-medium">{song.artist || i18n.t('collection.unknownArtist')}</p>
            </div>
            <div class="shrink-0 max-w-[45%] text-right">
              <p class="text-xs text-brand-text-secondary whitespace-normal">{song.genre || ""}</p>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="h-9 flex items-center justify-between mb-3">
      <div class="text-xs text-brand-text-secondary font-medium">
        {i18n.t("songTags.genresTabDescription", {}, "Browse songs by tags you've added")}
      </div>
      <div class="inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1">
        <button
          onclick={() => setViewMode("genre")}
          class="px-3 h-7 rounded-full text-xs font-semibold transition-colors {prefs.genreViewMode === 'genre' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          aria-pressed={prefs.genreViewMode === "genre"}
        >
          {i18n.t("songTags.viewGenre", {}, "Genre")}
        </button>
        <button
          onclick={() => setViewMode("tags")}
          class="px-3 h-7 rounded-full text-xs font-semibold transition-colors {prefs.genreViewMode === 'tags' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          aria-pressed={prefs.genreViewMode === "tags"}
        >
          {i18n.t("songTags.viewTags", {}, "Tags")}
        </button>
      </div>
    </div>

    {#if tagsStore.allTags.length === 0}
      <div class="py-16">
        <EmptyState
          icon={TagIcon}
          title={i18n.t("songTags.emptyTitle", {}, "No tags yet")}
          subtitle={i18n.t(
            "songTags.emptySubtitle",
            {},
            "Right-click a song and choose Edit Tags to give it a genre — the first value is its main category, the rest are subgenres."
          )}
        />
      </div>
    {:else if prefs.genreViewMode === "genre"}
      <div class="flex flex-col gap-1.5">
        {#each tagsStore.genreGraph as group (group.main_tag)}
          <div class="rounded-lg bg-brand-sidebar border border-brand-border/60 overflow-hidden">
            <div class="flex items-center">
              {#if group.children.length > 0}
                <button
                  onclick={() => toggleExpanded(group.main_tag)}
                  class="p-2.5 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
                  aria-label={expandedRoots.has(group.main_tag) ? i18n.t("common.collapse", {}, "Collapse") : i18n.t("common.expand", {}, "Expand")}
                >
                  {#if expandedRoots.has(group.main_tag)}
                    <ChevronDown class="w-4 h-4" />
                  {:else}
                    <ChevronRight class="w-4 h-4" />
                  {/if}
                </button>
              {:else}
                <span class="w-9"></span>
              {/if}
              <button
                onclick={() => openTag(group.main_tag)}
                class="flex-1 flex items-center justify-between py-2.5 pr-3 text-left"
              >
                <span class="text-sm font-semibold text-brand-text-primary">{group.main_tag}</span>
                <span class="text-xs text-brand-text-secondary tabular-nums">
                  {i18n.t("songTags.songCount", { count: group.song_count }, `${group.song_count} songs`)}
                </span>
              </button>
            </div>
            {#if expandedRoots.has(group.main_tag) && group.children.length > 0}
              <div class="pl-9 pb-1.5 flex flex-col">
                {#each group.children as child (child.name)}
                  <button
                    onclick={() => openTag(child.name)}
                    class="flex items-center justify-between py-1.5 pr-3 text-left text-brand-text-secondary hover:text-brand-text-primary transition-colors"
                  >
                    <span class="text-xs font-medium">{child.name}</span>
                    <span class="text-xs tabular-nums">
                      {i18n.t("songTags.songCount", { count: child.song_count }, `${child.song_count} songs`)}
                    </span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="flex flex-col gap-1.5">
        {#each tagsStore.allTags as tag (tag.name)}
          <button
            onclick={() => openTag(tag.name)}
            class="flex items-center justify-between px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 hover:border-brand-accent/60 transition-colors text-left"
          >
            <span class="text-sm font-semibold text-brand-text-primary">{tag.name}</span>
            <span class="text-xs text-brand-text-secondary tabular-nums">
              {i18n.t("songTags.songCount", { count: tag.song_count }, `${tag.song_count} songs`)}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
</div>

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    onPlay={() => playerStore.playSong(song.id)}
    onGoToArtist={() => collectionStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => collectionStore.viewAlbum(song.album || "")}
    onClose={() => { contextMenuState = null; }}
  />
{/if}
