<script lang="ts">
  import { collectionStore } from "$lib/stores/collection.svelte";
  import { i18n } from "$lib/stores/i18n.svelte";
  import { Columns } from "lucide-svelte";

  let showMenu = $state(false);

  function handleWindowClick(e: MouseEvent) {
    if (showMenu) {
      const target = e.target as HTMLElement | null;
      if (!target?.closest?.(".column-selector-container")) {
        showMenu = false;
      }
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div class="relative column-selector-container">
  <button
    onclick={() => (showMenu = !showMenu)}
    class="flex items-center gap-1.5 bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none transition-all cursor-pointer font-medium"
    title={i18n.t("collection.columnsBtn")}
  >
    <Columns class="w-3.5 h-3.5 text-brand-accent-text" />
    <span>{i18n.t("collection.columnsBtn")}</span>
  </button>

  {#if showMenu}
    <div
      class="absolute right-0 top-full mt-2 bg-brand-sidebar border border-brand-border rounded-xl shadow-2xl p-3 z-50 w-52 flex flex-col gap-1.5 select-none"
    >
      <div
        class="text-[11px] font-bold text-brand-text-secondary uppercase tracking-wider px-2 pb-1 border-b border-brand-border/40"
      >
        {i18n.t("collection.visibleColumnsHeader")}
      </div>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.format}
          onchange={() => collectionStore.toggleColumn("format")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnFormat")}
      </label>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.year}
          onchange={() => collectionStore.toggleColumn("year")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnYear")}
      </label>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.genre}
          onchange={() => collectionStore.toggleColumn("genre")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnGenre")}
      </label>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.bitrate}
          onchange={() => collectionStore.toggleColumn("bitrate")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnBitrate")}
      </label>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.rating}
          onchange={() => collectionStore.toggleColumn("rating")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnRating")}
      </label>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.playcount}
          onchange={() => collectionStore.toggleColumn("playcount")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnPlayCount")}
      </label>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.skipcount}
          onchange={() => collectionStore.toggleColumn("skipcount")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnSkipCount")}
      </label>
      <label
        class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary"
      >
        <input
          type="checkbox"
          checked={collectionStore.visibleColumns.duration}
          onchange={() => collectionStore.toggleColumn("duration")}
          class="rounded accent-brand-accent"
        />
        {i18n.t("collection.columnDuration")}
      </label>
    </div>
  {/if}
</div>
