<script lang="ts">
  import {
    PencilSimpleIcon as Pencil,
    ArrowLineUpIcon as ArrowUpToLine,
    TrashIcon as Trash2,
    ChartBarIcon as BarChart2
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  interface Props {
    x: number;
    y: number;
    name: string;
    /** True for a top-level card, false for a sub-genre chip — controls
     * whether "Promote to top-level genre" is offered. */
    isRoot: boolean;
    onRename: () => void;
    onPromote?: () => void;
    onDelete: () => void;
    onClose: () => void;
  }

  let { x, y, name, isRoot, onRename, onPromote, onDelete, onClose }: Props = $props();

  async function handleToggleStatsExcluded() {
    await statsExclusionsStore.toggleWithToast("genre", name);
  }
</script>

<ContextMenu {x} {y} {onClose}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {name}
  </div>

  <ContextMenuItem
    icon={Pencil}
    label={i18n.t("songTags.renameTag", {}, "Rename")}
    onclick={() => { onRename(); onClose(); }}
  />

  {#if !isRoot && onPromote}
    <ContextMenuItem
      icon={ArrowUpToLine}
      label={i18n.t("songTags.promoteTag", {}, "Promote to top-level genre")}
      onclick={() => { onPromote?.(); onClose(); }}
    />
  {/if}

  <ContextMenuItem
    icon={BarChart2}
    label={statsExclusionsStore.isExcluded("genre", name)
      ? i18n.t("stats.includeInStats")
      : i18n.t("stats.excludeFromStats")}
    onclick={() => { handleToggleStatsExcluded(); onClose(); }}
  />

  <ContextMenuDivider />

  <ContextMenuItem
    icon={Trash2}
    destructive
    label={i18n.t("songTags.deleteBtn", {}, "Delete")}
    onclick={() => { onDelete(); onClose(); }}
  />
</ContextMenu>
