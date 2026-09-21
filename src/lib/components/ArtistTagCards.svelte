<script lang="ts">
  import { DotsSixVerticalIcon as GripVertical } from "phosphor-svelte";
  import { tagsStore, type TagGroup, type TagGroupChild } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { genreColorHsl, getGenreColorChoices } from "../utils/genrePalette";
  import { portal } from "../utils/portal";
  import ArtistTagContextMenu from "./ArtistTagContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import ColorPicker from "./ColorPicker.svelte";

  const genreColorChoices = getGenreColorChoices();

  interface Props {
    hierarchy?: TagGroup[];
    onOpenTag: (tag: string) => void;
    sortField?: "name" | "count";
    sortAsc?: boolean;
    /** Collapses cards down to compact header-only rows (mirrors the
     * Albums/Artists cards-vs-rows toggle). */
    compact?: boolean;
  }

  let {
    hierarchy,
    onOpenTag,
    sortField = "name",
    sortAsc = true,
    compact = false,
  }: Props = $props();

  let effectiveHierarchy = $derived(hierarchy ?? tagsStore.artistHierarchy);

  let colorPopoverFor = $state<string | null>(null);
  let colorPopoverPos = $state<{ x: number; y: number } | null>(null);
  let colorPopoverEl = $state<HTMLDivElement | null>(null);

  function toggleColorPopover(e: MouseEvent, name: string) {
    if (colorPopoverFor === name) {
      colorPopoverFor = null;
      return;
    }
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    colorPopoverPos = { x: rect.left, y: rect.bottom + 4 };
    colorPopoverFor = name;
  }

  function handleWindowMouseDown(e: MouseEvent) {
    if (!colorPopoverFor) return;
    const target = e.target as HTMLElement;
    if (target.closest("[data-color-swatch-for]")) return;
    if (colorPopoverEl && !colorPopoverEl.contains(target)) {
      colorPopoverFor = null;
    }
  }

  let contextMenuTarget = $state<{ x: number; y: number; name: string; isRoot: boolean } | null>(null);
  let renamingTag = $state<string | null>(null);
  let renameValue = $state("");
  let deleteConfirmName = $state<string | null>(null);

  function openContextMenu(e: MouseEvent, name: string, isRoot: boolean) {
    e.preventDefault();
    contextMenuTarget = { x: e.clientX, y: e.clientY, name, isRoot };
  }

  function startRename(name: string) {
    renamingTag = name;
    renameValue = name;
  }

  function focusAndSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function commitRename() {
    const from = renamingTag;
    const into = renameValue.trim();
    renamingTag = null;
    if (!from || !into || into === from) return;
    const count = await tagsStore.mergeArtistTags(from, into);
    toastStore.show(
      i18n.t("songTags.artistRenameToast", { count, name: into }, `Renamed to "${into}" (${count} artists updated)`),
      "success"
    );
  }

  async function confirmDeleteTag() {
    const name = deleteConfirmName;
    deleteConfirmName = null;
    if (!name) return;
    const count = await tagsStore.deleteArtistTags([name]);
    toastStore.show(
      i18n.t("songTags.artistDeleteToast", { count }, `Deleted (${count} artists updated)`),
      "success"
    );
  }

  let sortedHierarchy = $derived.by(() => {
    const dir = sortAsc ? 1 : -1;
    const cmp = (a: { name: string; song_count: number }, b: { name: string; song_count: number }) =>
      sortField === "name" ? a.name.localeCompare(b.name) * dir : (a.song_count - b.song_count) * dir;
    return effectiveHierarchy
      .map((g) => ({ ...g, children: [...g.children].sort(cmp) }))
      .sort(cmp);
  });

  let draggedChip = $state<{ name: string; fromGroup: string } | null>(null);
  let draggedCard = $state<string | null>(null);
  let dropTarget = $state<{ kind: "card" | "header" | "chip"; group: string; chip?: string } | null>(null);
  let pointerPos = $state<{ x: number; y: number } | null>(null);
  const CLICK_VS_DRAG_THRESHOLD_PX = 4;
  let dragStartPos: { x: number; y: number } | null = null;
  let dragMoved = false;

  let ghostInfo = $derived.by(() => {
    if (draggedChip) {
      const group = effectiveHierarchy.find((g) => g.name === draggedChip!.fromGroup);
      return { label: draggedChip.name, colorIndex: group?.color_index ?? 0 };
    }
    if (draggedCard) {
      const group = effectiveHierarchy.find((g) => g.name === draggedCard);
      return { label: draggedCard, colorIndex: group?.color_index ?? 0 };
    }
    return null;
  });

  function handleChipPointerDown(e: PointerEvent, name: string, fromGroup: string) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    e.preventDefault();
    draggedChip = { name, fromGroup };
    dragStartPos = { x: e.clientX, y: e.clientY };
    dragMoved = false;
    pointerPos = { x: e.clientX, y: e.clientY };
  }

  function handleCardPointerDown(e: PointerEvent, name: string) {
    if (e.button !== 0) return;
    e.preventDefault();
    draggedCard = name;
    dragStartPos = { x: e.clientX, y: e.clientY };
    dragMoved = false;
    pointerPos = { x: e.clientX, y: e.clientY };
  }

  function handlePointerMove(e: PointerEvent) {
    if (!draggedChip && !draggedCard) return;
    pointerPos = { x: e.clientX, y: e.clientY };
    if (dragStartPos && !dragMoved) {
      const dx = e.clientX - dragStartPos.x;
      const dy = e.clientY - dragStartPos.y;
      if (Math.hypot(dx, dy) > CLICK_VS_DRAG_THRESHOLD_PX) dragMoved = true;
    }
    const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    const chipEl = el?.closest<HTMLElement>("[data-artist-chip-key]");
    const headerEl = el?.closest<HTMLElement>("[data-artist-card-header]");
    const cardEl = el?.closest<HTMLElement>("[data-artist-card-name]");
    if (chipEl) {
      dropTarget = { kind: "chip", group: chipEl.dataset.artistChipGroup!, chip: chipEl.dataset.artistChipKey! };
    } else if (headerEl) {
      dropTarget = { kind: "header", group: headerEl.dataset.artistCardHeader! };
    } else if (cardEl) {
      dropTarget = { kind: "card", group: cardEl.dataset.artistCardName! };
    } else {
      dropTarget = null;
    }
  }

  function handleCardClick(e: MouseEvent, name: string) {
    const target = e.target as HTMLElement;
    if (target.closest("[data-artist-chip-key], [data-color-swatch-for]")) return;
    if (target.tagName === "INPUT") return;
    onOpenTag(name);
  }

  async function handlePointerUp() {
    const chip = draggedChip;
    const card = draggedCard;
    const target = dropTarget;
    const moved = dragMoved;
    draggedChip = null;
    draggedCard = null;
    dropTarget = null;
    pointerPos = null;
    dragStartPos = null;
    dragMoved = false;

    if (card) {
      if (target && target.group !== card) {
        await tagsStore.demoteArtistGroupToChild(card, target.group);
      }
      return;
    }

    if (!chip) return;
    if (!moved) {
      onOpenTag(chip.name);
      return;
    }

    if (!target) return;
    if (target.kind === "header" && target.group === chip.fromGroup) {
      await tagsStore.promoteArtistTag(chip.name);
    } else if (target.kind === "card" && target.group !== chip.fromGroup) {
      await tagsStore.reparentArtistTag(chip.name, target.group);
    } else if (target.kind === "chip" && target.group === chip.fromGroup && target.chip !== chip.name) {
      const group = effectiveHierarchy.find((g) => g.name === chip.fromGroup);
      const newIndex = group?.children.findIndex((c: TagGroupChild) => c.name === target.chip) ?? 0;
      await tagsStore.reorderArtistTagInGroup(chip.name, newIndex);
    } else if (target.kind === "chip" && target.group !== chip.fromGroup) {
      await tagsStore.reparentArtistTag(chip.name, target.group);
    }
  }
</script>

<svelte:window onpointermove={handlePointerMove} onpointerup={handlePointerUp} onmousedown={handleWindowMouseDown} />

<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
  {#each sortedHierarchy as group (group.name)}
    {@const cardHighlighted = (dropTarget?.kind === 'card' || dropTarget?.kind === 'header') && dropTarget.group === group.name && (draggedChip || draggedCard)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      data-artist-card-name={group.name}
      onclick={(e) => handleCardClick(e, group.name)}
      class="rounded-lg bg-brand-sidebar border-2 overflow-hidden transition-[opacity,box-shadow,border-color,transform] cursor-pointer {draggedCard === group.name ? 'opacity-40' : ''} {cardHighlighted ? 'border-brand-accent ring-4 ring-brand-accent/50 scale-[1.02] bg-brand-accent/5' : ''}"
      style={cardHighlighted ? '' : `border-color: ${genreColorHsl(group.color_index)}`}
    >
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        data-artist-card-header={group.name}
        oncontextmenu={(e) => openContextMenu(e, group.name, true)}
        class="flex items-center gap-2 px-3 py-2.5 transition-colors {dropTarget?.kind === 'header' && dropTarget.group === group.name ? 'bg-brand-accent/25' : ''}"
      >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          onpointerdown={(e) => handleCardPointerDown(e, group.name)}
          class="shrink-0 touch-none artist-drag-handle {draggedCard === group.name ? 'is-dragging' : ''} text-brand-text-secondary/50 hover:text-brand-text-secondary"
          title={i18n.t("songTags.dragArtistCardTooltip", {}, "Drag to make this a sub-tag of another card")}
        >
          <GripVertical class="w-3.5 h-3.5" />
        </span>
        <div class="relative shrink-0">
          <button
            type="button"
            data-color-swatch-for={group.name}
            onclick={(e) => toggleColorPopover(e, group.name)}
            class="w-4 h-4 rounded-full border border-black/10 shrink-0"
            style="background-color: {genreColorHsl(group.color_index)}"
            title={i18n.t("songTags.changeColorTooltip", {}, "Change color")}
            aria-label={i18n.t("songTags.changeColorTooltip", {}, "Change color")}
          ></button>
        </div>
        {#if renamingTag === group.name}
          <input
            use:focusAndSelect
            bind:value={renameValue}
            onblur={commitRename}
            onkeydown={(e) => {
              if (e.key === "Enter") commitRename();
              if (e.key === "Escape") renamingTag = null;
            }}
            class="flex-1 min-w-0 bg-brand-main border border-brand-accent rounded px-1.5 py-0.5 text-sm font-semibold text-brand-text-primary"
          />
        {:else}
          <button
            type="button"
            onclick={() => onOpenTag(group.name)}
            class="flex-1 min-w-0 flex items-center justify-between gap-2 text-left"
          >
            <span class="text-sm font-semibold text-brand-text-primary truncate">{group.name}</span>
            <span class="text-xs text-brand-text-secondary tabular-nums shrink-0">
              {i18n.t("songTags.artistCount", { count: group.song_count }, group.song_count === 1 ? "1 artist" : `${group.song_count} artists`)}
            </span>
          </button>
        {/if}
      </div>

      <div class="px-3 pb-3 flex flex-wrap gap-1.5 min-h-9 {compact ? 'hidden' : ''}">
        {#if group.children.length === 0}
          <p class="text-xs text-brand-text-secondary/70 italic py-1">
            {i18n.t("songTags.noArtistSubtagsYet", {}, "No sub-tags yet — drag a tag here")}
          </p>
        {/if}
        {#each group.children as child (child.name)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            data-artist-chip-key={child.name}
            data-artist-chip-group={group.name}
            onpointerdown={(e) => handleChipPointerDown(e, child.name, group.name)}
            oncontextmenu={(e) => openContextMenu(e, child.name, false)}
            class="inline-flex items-center gap-1 pl-2 pr-1.5 py-0.5 rounded-full border-2 bg-[color-mix(in_srgb,var(--color-brand-accent)_15%,var(--color-brand-sidebar))] text-brand-text-primary text-xs font-medium select-none touch-none transition-[opacity,box-shadow,transform] artist-drag-handle {draggedChip?.name === child.name ? 'is-dragging opacity-40' : ''} {dropTarget?.kind === 'chip' && dropTarget.chip === child.name ? 'ring-4 ring-brand-accent scale-110' : ''}"
            style={`border-color: ${genreColorHsl(group.color_index)};`}
          >
            {#if renamingTag === child.name}
              <input
                use:focusAndSelect
                bind:value={renameValue}
                onblur={commitRename}
                onkeydown={(e) => {
                  if (e.key === "Enter") commitRename();
                  if (e.key === "Escape") renamingTag = null;
                }}
                onclick={(e) => e.stopPropagation()}
                class="w-24 bg-brand-main border border-brand-accent rounded px-1 text-brand-text-primary"
              />
            {:else}
              <span class="inline-flex items-baseline gap-1">
                <span>{child.name}</span>
                <span class="text-[0.85em] opacity-70">{child.song_count}</span>
              </span>
            {/if}
          </span>
        {/each}
      </div>
    </div>
  {/each}
</div>

{#if ghostInfo && pointerPos}
  <div
    class="fixed z-50 pointer-events-none px-3 py-1.5 rounded-full border-2 bg-[color-mix(in_srgb,var(--color-brand-accent)_15%,var(--color-brand-sidebar))] text-brand-text-primary text-xs font-semibold shadow-2xl -translate-y-1/2"
    style={`left: ${pointerPos.x + 16}px; top: ${pointerPos.y}px; border-color: ${genreColorHsl(ghostInfo.colorIndex)};`}
  >
    {ghostInfo.label}
  </div>
{/if}

{#if colorPopoverFor && colorPopoverPos}
  {@const group = effectiveHierarchy.find((g) => g.name === colorPopoverFor)}
  <div
    use:portal
    bind:this={colorPopoverEl}
    class="fixed z-50 p-2 rounded-lg bg-brand-main border border-brand-border shadow-2xl"
    style={`left: ${colorPopoverPos.x}px; top: ${colorPopoverPos.y}px;`}
  >
    <ColorPicker
      choices={genreColorChoices}
      value={group ? String(group.color_index) : null}
      onChange={(v) => { tagsStore.setArtistGroupColor(colorPopoverFor!, Number(v)); colorPopoverFor = null; }}
      size="sm"
      columns={5}
    />
  </div>
{/if}

{#if contextMenuTarget}
  <ArtistTagContextMenu
    x={contextMenuTarget.x}
    y={contextMenuTarget.y}
    name={contextMenuTarget.name}
    isRoot={contextMenuTarget.isRoot}
    onRename={() => startRename(contextMenuTarget!.name)}
    onPromote={contextMenuTarget.isRoot ? undefined : () => tagsStore.promoteArtistTag(contextMenuTarget!.name)}
    onDelete={() => { deleteConfirmName = contextMenuTarget!.name; }}
    onClose={() => { contextMenuTarget = null; }}
  />
{/if}

{#if deleteConfirmName}
  <ConfirmDialog
    title={i18n.t("songTags.deleteBtn", {}, "Delete")}
    message={i18n.t(
      "songTags.artistDeleteConfirmMessage",
      { count: 1 },
      `Remove "${deleteConfirmName}" from every artist that carries it? This can't be undone.`
    )}
    confirmLabel={i18n.t("songTags.deleteBtn", {}, "Delete")}
    cancelLabel={i18n.t("songTags.cancelBtn", {}, "Cancel")}
    onConfirm={confirmDeleteTag}
    onCancel={() => { deleteConfirmName = null; }}
  />
{/if}

<style>
  .artist-drag-handle,
  .artist-drag-handle :global(*:not(input)) {
    cursor: grab !important;
  }
  .artist-drag-handle.is-dragging,
  .artist-drag-handle.is-dragging :global(*:not(input)) {
    cursor: grabbing !important;
  }
</style>
