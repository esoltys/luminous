<script lang="ts">
  import {
    PencilSimpleIcon as Pencil,
    ArrowLineUpIcon as ArrowUpToLine,
    TrashIcon as Trash2
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  interface Props {
    x: number;
    y: number;
    name: string;
    /** True for a top-level card, false for a sub-tag chip — controls
     * whether "Promote to top-level tag" is offered. */
    isRoot: boolean;
    onRename: () => void;
    onPromote?: () => void;
    onDelete: () => void;
    onClose: () => void;
  }

  let { x, y, name, isRoot, onRename, onPromote, onDelete, onClose }: Props = $props();
</script>

<ContextMenu {x} {y} {onClose} estimatedHeight={isRoot ? 100 : 140}>
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
      label={i18n.t("songTags.promoteArtistTag", {}, "Promote to top-level tag")}
      onclick={() => { onPromote?.(); onClose(); }}
    />
  {/if}

  <ContextMenuDivider />

  <ContextMenuItem
    icon={Trash2}
    destructive
    label={i18n.t("songTags.deleteBtn", {}, "Delete")}
    onclick={() => { onDelete(); onClose(); }}
  />
</ContextMenu>
