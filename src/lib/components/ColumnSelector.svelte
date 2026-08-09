<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import type { VisibleColumns } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { portal } from "../utils/portal";
  import { SONG_TABLE_COLUMNS } from "../utils/songColumns";
  import { Columns } from "lucide-svelte";

  interface Props {
    align?: "left" | "right";
    size?: "sm" | "md";
    /** Renders just the icon in a round button, dropping the "Columns" label — for compact toolbars. */
    iconOnly?: boolean;
  }

  let { align = "right", size = "md", iconOnly = false }: Props = $props();

  let showMenu = $state(false);
  let buttonEl = $state<HTMLButtonElement | undefined>(undefined);

  const MENU_WIDTH_PX = 256; // w-64
  const MENU_GAP_PX = 8; // mt-2
  const MENU_MAX_HEIGHT_PX = 448; // 28rem ceiling
  const MENU_MIN_HEIGHT_PX = 160;
  // Matches the pb-28 bottom padding views reserve elsewhere to clear the floating PlayerBar dock.
  const PLAYERBAR_RESERVE_PX = 112;
  const PAGE_BOTTOM_MARGIN_PX = 16;

  let menuMaxHeight = $state(MENU_MAX_HEIGHT_PX);
  let menuTop = $state(0);
  let menuLeft = $state(0);

  // The trigger can sit inside an `overflow-hidden` hero banner, which would
  // otherwise clip the dropdown once it extends past the banner's edge — so
  // the menu is portaled to <body> and positioned with fixed viewport coords
  // computed from the trigger's own position instead of relying on a
  // CSS-relative `position: absolute` container.
  function updateMenuPosition() {
    if (!buttonEl) return;
    const rect = buttonEl.getBoundingClientRect();
    const reserve = playerStore.currentSong ? PLAYERBAR_RESERVE_PX : PAGE_BOTTOM_MARGIN_PX;
    const available = window.innerHeight - rect.bottom - MENU_GAP_PX - reserve;
    menuMaxHeight = Math.max(MENU_MIN_HEIGHT_PX, Math.min(MENU_MAX_HEIGHT_PX, available));
    menuTop = rect.bottom + MENU_GAP_PX;
    menuLeft = align === "left"
      ? rect.left
      : Math.max(8, rect.right - MENU_WIDTH_PX);
  }

  function toggleMenu() {
    showMenu = !showMenu;
    if (showMenu) updateMenuPosition();
  }

  function handleWindowClick(e: MouseEvent) {
    if (showMenu) {
      const target = e.target as HTMLElement | null;
      if (!target?.closest?.(".column-selector-container, .column-selector-menu")) {
        showMenu = false;
      }
    }
  }

  // Ordered list of all columns as they appear in the table (left-to-right)
  const TABLE_ORDER: { key: keyof VisibleColumns; label: string }[] = SONG_TABLE_COLUMNS;

  // Metatag columns (embedded in audio file), alphabetical
  const METATAG_COLS: { key: keyof VisibleColumns; label: string }[] = [
    { key: "album",       label: "collection.columnAlbum" },
    { key: "album_artist",label: "collection.columnAlbumArtist" },
    { key: "artist",      label: "collection.columnArtist" },
    { key: "bitdepth",    label: "collection.columnBitDepth" },
    { key: "bitrate",     label: "collection.columnBitrate" },
    { key: "bpm",         label: "collection.columnBpm" },
    { key: "channels",    label: "collection.columnChannels" },
    { key: "composer",    label: "collection.columnComposer" },
    { key: "filesize",    label: "collection.columnFileSize" },
    { key: "format",      label: "collection.columnFormat" },
    { key: "genre",       label: "collection.columnGenre" },
    { key: "grouping",    label: "collection.columnGrouping" },
    { key: "initial_key", label: "collection.columnInitialKey" },
    { key: "path",        label: "collection.columnPath" },
    { key: "samplerate",  label: "collection.columnSampleRate" },
    { key: "title",       label: "collection.columnTitle" },
    { key: "track",       label: "collection.columnTrack" },
    { key: "year",        label: "collection.columnYear" },
  ];

  // Luminous-derived columns (not stored in file tags), alphabetical
  const LUMINOUS_COLS: { key: keyof VisibleColumns; label: string }[] = [
    { key: "actions",   label: "collection.columnActions" },
    { key: "added",     label: "collection.columnAdded" },
    { key: "duration",  label: "collection.columnDuration" },
    { key: "lastplayed",label: "collection.columnLastPlayed" },
    { key: "playcount", label: "collection.columnPlayCount" },
    { key: "rating",    label: "collection.columnRating" },
    { key: "skipcount", label: "collection.columnSkipCount" },
  ];

  let visibleInOrder = $derived(
    TABLE_ORDER.filter(col => collectionStore.visibleColumns[col.key])
  );
</script>

<svelte:window onclick={handleWindowClick} onresize={() => showMenu && updateMenuPosition()} />

<div class="relative column-selector-container shrink-0 z-40">
  <button
    bind:this={buttonEl}
    onclick={toggleMenu}
    class="flex items-center justify-center gap-2 border border-brand-border text-brand-text-secondary focus:outline-none transition-colors font-semibold rounded-full
      {iconOnly
        ? 'w-10 h-10 hover:text-brand-accent-text hover:bg-brand-sidebar shadow-xs'
        : 'bg-brand-sidebar hover:bg-brand-main hover:text-brand-text-primary transition-all ' + (size === 'sm' ? 'px-2.5 h-7 text-[11px] gap-1.5' : 'px-5 py-2 text-sm')}"
    title={i18n.t("collection.columnsBtn")}
  >
    <Columns class={iconOnly || size === 'sm' ? 'w-3.5 h-3.5' : 'w-4 h-4'} />
    {#if !iconOnly}<span>{i18n.t("collection.columnsBtn")}</span>{/if}
  </button>

  {#if showMenu}
    <div
      use:portal
      class="column-selector-menu fixed bg-brand-sidebar border border-brand-border rounded-xl shadow-2xl p-3 z-50 w-64 overflow-y-auto flex flex-col gap-0.5 select-none custom-scrollbar"
      style="top: {menuTop}px; left: {menuLeft}px; max-height: {menuMaxHeight}px"
    >
      <div class="text-[10px] font-extrabold text-brand-accent-text uppercase tracking-wider px-2 pt-1 pb-0.5">
        Visible
      </div>
      {#if visibleInOrder.length === 0}
        <div class="px-2 py-2 text-[11px] text-brand-text-secondary/60 italic">No columns visible</div>
      {:else}
        {#each visibleInOrder as col (col.key)}
          <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs text-brand-text-primary">
            <input type="checkbox" checked={collectionStore.visibleColumns[col.key]} onchange={() => collectionStore.toggleColumn(col.key)} class="rounded accent-brand-accent" />
            {i18n.t(col.label)}
          </label>
        {/each}
      {/if}

      <div class="text-[10px] font-extrabold text-brand-accent-text uppercase tracking-wider px-2 pt-2 pb-0.5 mt-1 border-t border-brand-border/30">
        Metatags
      </div>
      {#each METATAG_COLS as col (col.key)}
        <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs {collectionStore.visibleColumns[col.key] ? 'text-brand-text-primary' : 'text-brand-text-secondary/70'}">
          <input type="checkbox" checked={collectionStore.visibleColumns[col.key]} onchange={() => collectionStore.toggleColumn(col.key)} class="rounded accent-brand-accent" />
          {i18n.t(col.label)}
        </label>
      {/each}

      <div class="text-[10px] font-extrabold text-brand-accent-text uppercase tracking-wider px-2 pt-2 pb-0.5 mt-1 border-t border-brand-border/30">
        Luminous
      </div>
      {#each LUMINOUS_COLS as col (col.key)}
        <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs {collectionStore.visibleColumns[col.key] ? 'text-brand-text-primary' : 'text-brand-text-secondary/70'}">
          <input type="checkbox" checked={collectionStore.visibleColumns[col.key]} onchange={() => collectionStore.toggleColumn(col.key)} class="rounded accent-brand-accent" />
          {i18n.t(col.label)}
        </label>
      {/each}
    </div>
  {/if}
</div>
