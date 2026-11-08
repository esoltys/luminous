<script lang="ts">
  import { songDrag } from "../stores/songDrag.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import DragGhost from "./DragGhost.svelte";
  import {
    DeviceMobileSpeakerIcon,
    MusicNotesIcon as Music,
    PlaylistIcon as ListMusic,
    StackIcon as Layers,
  } from "phosphor-svelte";
</script>

{#if songDrag.payload && songDrag.pointerPos}
  <DragGhost
    x={songDrag.pointerPos.x}
    y={songDrag.pointerPos.y}
    overTarget={songDrag.dropTarget !== null}
    testId="song-drag-ghost"
  >
    <Music class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    <span class="max-w-[180px] truncate">{songDrag.payload.label}</span>
    {#if songDrag.dropTarget}
      <span class="text-brand-text-secondary text-[10px]">→</span>
      {#if songDrag.dropTarget.kind === "queue"}
        <span class="inline-flex items-center gap-1 font-semibold text-brand-accent-text">
          <Layers class="w-3.5 h-3.5" />
          <span>{i18n.t("playerBar.queueTitle", {}, "Queue")}</span>
        </span>
      {:else if songDrag.dropTarget.kind === "custom-category"}
        <span class="inline-flex items-center gap-1 font-semibold text-brand-accent-text">
          <ListMusic class="w-3.5 h-3.5" />
          <span>{i18n.t("sidebar.playlistsCustom", {}, "Custom")}</span>
        </span>
      {:else if songDrag.dropTarget.kind === "playlist"}
        <span class="inline-flex items-center gap-1 font-semibold text-brand-accent-text max-w-[140px] truncate">
          <ListMusic class="w-3.5 h-3.5 shrink-0" />
          <span class="truncate">{songDrag.dropTarget.name}</span>
        </span>
      {:else if songDrag.dropTarget.kind === "device"}
        <span class="inline-flex items-center gap-1 font-semibold text-brand-accent-text max-w-[140px] truncate">
          <DeviceMobileSpeakerIcon class="w-3.5 h-3.5 shrink-0" />
          <span class="truncate">{songDrag.dropTarget.name}</span>
        </span>
      {/if}
    {/if}
  </DragGhost>
{/if}
