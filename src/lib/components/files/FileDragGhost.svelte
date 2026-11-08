<script lang="ts">
  import { fileDrag } from "../../stores/fileDrag.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import DragGhost from "../DragGhost.svelte";
  import {
    MusicNotesIcon as Music,
    PlaylistIcon as ListMusic,
    StackIcon as Layers,
  } from "phosphor-svelte";

  const ghostLabel = $derived.by(() => {
    const count = fileDrag.draggedEntries.length;
    if (count === 1) {
      return fileDrag.draggedEntries[0]?.name ?? "";
    }
    return i18n.plural("files.songCount", count);
  });
</script>

{#if fileDrag.isDragging && fileDrag.pointerPos}
  <DragGhost
    x={fileDrag.pointerPos.x}
    y={fileDrag.pointerPos.y}
    overTarget={fileDrag.dropTarget !== null}
    testId="file-drag-ghost"
  >
    <Music class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    <span class="max-w-[180px] truncate">{ghostLabel}</span>
    {#if fileDrag.dropTarget}
      <span class="text-brand-text-secondary text-[10px]">→</span>
      {#if fileDrag.dropTarget.kind === "queue"}
        <span class="inline-flex items-center gap-1 font-semibold text-brand-accent-text">
          <Layers class="w-3.5 h-3.5" />
          <span>{i18n.t("playerBar.queueTitle", {}, "Queue")}</span>
        </span>
      {:else if fileDrag.dropTarget.kind === "custom-category"}
        <span class="inline-flex items-center gap-1 font-semibold text-brand-accent-text">
          <ListMusic class="w-3.5 h-3.5" />
          <span>{i18n.t("sidebar.playlistsCustom", {}, "Custom")}</span>
        </span>
      {:else if fileDrag.dropTarget.kind === "playlist"}
        <span class="inline-flex items-center gap-1 font-semibold text-brand-accent-text max-w-[140px] truncate">
          <ListMusic class="w-3.5 h-3.5 shrink-0" />
          <span class="truncate">{fileDrag.dropTarget.name}</span>
        </span>
      {/if}
    {/if}
  </DragGhost>
{/if}
