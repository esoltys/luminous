<script lang="ts">
  import { AlertTriangle, GripVertical } from "lucide-svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { GENRE_PALETTE_HUES, genreColorHsl } from "../utils/genrePalette";
  import { portal } from "../utils/portal";
  import GenreContextMenu from "./GenreContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  interface Props {
    onOpenMainTag: (tag: string) => void;
    onOpenGenreEdge: (root: string, child: string) => void;
    selectMode: boolean;
    selected: Set<string>;
    onToggleSelect: (name: string) => void;
    sortField?: "name" | "count";
    sortAsc?: boolean;
    /** Collapses cards down to compact header-only rows (mirrors the
     * Albums/Artists cards-vs-rows toggle). */
    compact?: boolean;
  }

  let {
    onOpenMainTag,
    onOpenGenreEdge,
    selectMode,
    selected,
    onToggleSelect,
    sortField = "name",
    sortAsc = true,
    compact = false,
  }: Props = $props();

  // Portaled to document.body (see the imported `portal` action) rather than
  // positioned absolute inside the card — the card has overflow-hidden (so
  // its header's flush corners match the rounded border), which otherwise
  // clips the popover to a sliver instead of just placing it above other
  // content.
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
    // The swatch button that opened it toggles the popover itself in its
    // own click handler — closing here first would just make it reopen.
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

  /** Renaming a tag is just a merge where the destination name doesn't have
   * to already exist — mergeTags rewrites every song's embedded/DB genre
   * text from `from` to `into` regardless. */
  async function commitRename() {
    const from = renamingTag;
    const into = renameValue.trim();
    renamingTag = null;
    if (!from || !into || into === from) return;
    const count = await tagsStore.mergeTags(from, into);
    toastStore.show(
      i18n.t("songTags.renameToast", { count, name: into }, `Renamed to "${into}" (${count} songs updated)`),
      "success"
    );
  }

  async function confirmDeleteTag() {
    const name = deleteConfirmName;
    deleteConfirmName = null;
    if (!name) return;
    const count = await tagsStore.deleteTags([name]);
    toastStore.show(
      i18n.t("songTags.deleteToast", { count }, `Deleted (${count} songs updated)`),
      "success"
    );
  }

  /** Sorts both the cards themselves and each card's own children — display
   * only, doesn't touch the persisted drag-reorder sort_order. */
  let sortedHierarchy = $derived.by(() => {
    const dir = sortAsc ? 1 : -1;
    const cmp = (a: { name: string; song_count: number }, b: { name: string; song_count: number }) =>
      sortField === "name" ? a.name.localeCompare(b.name) * dir : (a.song_count - b.song_count) * dir;
    return tagsStore.hierarchy
      .map((g) => ({ ...g, children: [...g.children].sort(cmp) }))
      .sort(cmp);
  });

  // Drag reparent/promote/reorder cannot use native HTML5 drag-and-drop —
  // Tauri intercepts OS-level drag-drop at the webview layer so `dragstart`
  // never fires in-page (see ChipInput.svelte). Mirrors its pointer-event
  // pattern instead.
  let draggedChip = $state<{ name: string; fromGroup: string } | null>(null);
  /** Whole-card drag (via the header's grip handle) — demotes a top-level
   * card into a sub-genre chip under whatever card it's dropped on. Shares
   * the same dropTarget tracking as chip drags since hit-testing doesn't
   * care which kind of drag is in progress. */
  let draggedCard = $state<string | null>(null);
  let dropTarget = $state<{ kind: "card" | "header" | "chip"; group: string; chip?: string } | null>(null);
  /** Cursor position while a drag is active, for the floating ghost label —
   * cleared alongside draggedChip/draggedCard on pointerup. */
  let pointerPos = $state<{ x: number; y: number } | null>(null);
  /** Distinguishes a click from a drag on the same pointerdown/up pair — a
   * chip's whole surface (not just its edge padding) needs to be grabbable,
   * so click-to-open can no longer be a separate <button> the drag simply
   * avoids; instead, movement past this threshold before pointerup is what
   * makes it a drag rather than a click. */
  const CLICK_VS_DRAG_THRESHOLD_PX = 4;
  let dragStartPos: { x: number; y: number } | null = null;
  let dragMoved = false;

  /** What the floating ghost shows: the dragged item's own label, colored
   * like its current card (falls back to the target card's own hue for a
   * promoted-from-nowhere case, though that shouldn't normally happen). */
  let ghostInfo = $derived.by(() => {
    if (draggedChip) {
      const group = tagsStore.hierarchy.find((g) => g.name === draggedChip!.fromGroup);
      return { label: draggedChip.name, colorIndex: group?.color_index ?? 0 };
    }
    if (draggedCard) {
      const group = tagsStore.hierarchy.find((g) => g.name === draggedCard);
      return { label: draggedCard, colorIndex: group?.color_index ?? 0 };
    }
    return null;
  });

  function handleChipPointerDown(e: PointerEvent, name: string, fromGroup: string) {
    if (selectMode) return;
    // The rename input and the a11y checkbox handle their own clicks —
    // don't hijack them into a drag.
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    e.preventDefault();
    draggedChip = { name, fromGroup };
    dragStartPos = { x: e.clientX, y: e.clientY };
    dragMoved = false;
    pointerPos = { x: e.clientX, y: e.clientY };
  }

  function handleCardPointerDown(e: PointerEvent, name: string) {
    if (selectMode) return;
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
    const chipEl = el?.closest<HTMLElement>("[data-chip-key]");
    const headerEl = el?.closest<HTMLElement>("[data-card-header]");
    const cardEl = el?.closest<HTMLElement>("[data-card-name]");
    if (chipEl) {
      dropTarget = { kind: "chip", group: chipEl.dataset.chipGroup!, chip: chipEl.dataset.chipKey! };
    } else if (headerEl) {
      dropTarget = { kind: "header", group: headerEl.dataset.cardHeader! };
    } else if (cardEl) {
      dropTarget = { kind: "card", group: cardEl.dataset.cardName! };
    } else {
      dropTarget = null;
    }
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
        await tagsStore.demoteGroupToChild(card, target.group);
      }
      return;
    }

    if (!chip) return;
    // Barely moved — treat the whole gesture as a plain click on the chip
    // rather than a drag, so the entire pill (not just its edge) opens the
    // drill-down view.
    if (!moved) {
      onOpenGenreEdge(chip.fromGroup, chip.name);
      return;
    }

    if (!target) return;
    if (target.kind === "header" && target.group === chip.fromGroup) {
      await tagsStore.promoteTag(chip.name);
    } else if (target.kind === "card" && target.group !== chip.fromGroup) {
      await tagsStore.reparentTag(chip.name, target.group);
    } else if (target.kind === "chip" && target.group === chip.fromGroup && target.chip !== chip.name) {
      const group = tagsStore.hierarchy.find((g) => g.name === chip.fromGroup);
      const newIndex = group?.children.findIndex((c) => c.name === target.chip) ?? 0;
      await tagsStore.reorderTagInGroup(chip.name, newIndex);
    } else if (target.kind === "chip" && target.group !== chip.fromGroup) {
      await tagsStore.reparentTag(chip.name, target.group);
    }
  }
</script>

<svelte:window onpointermove={handlePointerMove} onpointerup={handlePointerUp} onmousedown={handleWindowMouseDown} />

<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
  {#each sortedHierarchy as group (group.name)}
    <div
      data-card-name={group.name}
      class="rounded-lg bg-brand-sidebar border overflow-hidden transition-[opacity,box-shadow,border-color,transform] {draggedCard === group.name ? 'opacity-40' : ''} {(dropTarget?.kind === 'card' || dropTarget?.kind === 'header') && dropTarget.group === group.name && (draggedChip || draggedCard) ? 'border-brand-accent ring-4 ring-brand-accent/50 scale-[1.02] bg-brand-accent/5' : 'border-brand-border/60'}"
    >
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        data-card-header={group.name}
        oncontextmenu={(e) => openContextMenu(e, group.name, true)}
        class="flex items-center gap-2 px-3 py-2.5 transition-colors {dropTarget?.kind === 'header' && dropTarget.group === group.name ? 'bg-brand-accent/25' : ''}"
      >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          onpointerdown={(e) => handleCardPointerDown(e, group.name)}
          class="shrink-0 touch-none {selectMode ? '' : 'cursor-grab active:cursor-grabbing'} text-brand-text-secondary/50 hover:text-brand-text-secondary"
          title={i18n.t("songTags.dragCardTooltip", {}, "Drag to make this a sub-genre of another card")}
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
            onclick={() => onOpenMainTag(group.name)}
            class="flex-1 min-w-0 flex items-center justify-between gap-2 text-left"
          >
            <span class="text-sm font-semibold text-brand-text-primary truncate">{group.name}</span>
            <span class="text-xs text-brand-text-secondary tabular-nums shrink-0">
              {i18n.t("songTags.songCount", { count: group.song_count }, `${group.song_count} songs`)}
            </span>
          </button>
        {/if}
      </div>

      <div class="px-3 pb-3 flex flex-wrap gap-1.5 min-h-9 {compact ? 'hidden' : ''}">
        {#if group.children.length === 0}
          <p class="text-xs text-brand-text-secondary/70 italic py-1">
            {i18n.t("songTags.noSubgenresYet", {}, "No sub-genres yet — drag a tag here")}
          </p>
        {/if}
        {#each group.children as child (child.name)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            data-chip-key={child.name}
            data-chip-group={group.name}
            onpointerdown={(e) => handleChipPointerDown(e, child.name, group.name)}
            onclick={() => { if (selectMode) onToggleSelect(child.name); }}
            oncontextmenu={(e) => openContextMenu(e, child.name, false)}
            class="inline-flex items-center gap-1 pl-2 pr-1.5 py-1 rounded-full border text-xs font-medium select-none touch-none transition-[opacity,box-shadow,transform] {selectMode ? 'cursor-pointer' : 'cursor-grab active:cursor-grabbing'} {draggedChip?.name === child.name ? 'opacity-40' : ''} {dropTarget?.kind === 'chip' && dropTarget.chip === child.name ? 'ring-4 ring-brand-accent scale-110' : selected.has(child.name) ? 'ring-2 ring-brand-accent' : ''}"
            style={`background-color: color-mix(in srgb, ${genreColorHsl(group.color_index)} 22%, transparent); border-color: color-mix(in srgb, ${genreColorHsl(group.color_index)} 45%, transparent); color: ${genreColorHsl(group.color_index)};`}
          >
            {#if selectMode}
              <input
                type="checkbox"
                checked={selected.has(child.name)}
                onchange={() => onToggleSelect(child.name)}
                class="w-3 h-3 pointer-events-none"
              />
            {/if}
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
              <!-- Click-to-open is handled by handlePointerUp (a chip
                   pointerdown/up with no meaningful movement) so the whole
                   pill is grabbable for dragging, not just a <button>'s
                   edge padding around the label. -->
              <span class="inline-flex items-baseline gap-1">
                <span>{child.name}</span>
                <span class="text-[0.85em] opacity-70">{child.song_count}</span>
              </span>
            {/if}
            {#if child.is_conflict}
              <span title={i18n.t("songTags.conflictTooltip", { name: child.name }, `"${child.name}" is also a top-level genre elsewhere in your library`)}>
                <AlertTriangle class="w-3 h-3 text-amber-400" />
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
    class="fixed z-50 pointer-events-none px-3 py-1.5 rounded-full border text-xs font-semibold shadow-2xl -translate-y-1/2"
    style={`left: ${pointerPos.x + 16}px; top: ${pointerPos.y}px; background-color: color-mix(in srgb, ${genreColorHsl(ghostInfo.colorIndex)} 40%, var(--color-brand-sidebar)); color: color-mix(in srgb, ${genreColorHsl(ghostInfo.colorIndex)} 90%, var(--color-brand-text-primary)); border-color: color-mix(in srgb, ${genreColorHsl(ghostInfo.colorIndex)} 60%, transparent);`}
  >
    {ghostInfo.label}
  </div>
{/if}

{#if colorPopoverFor && colorPopoverPos}
  {@const group = sortedHierarchy.find((g) => g.name === colorPopoverFor)}
  <div
    use:portal
    bind:this={colorPopoverEl}
    class="fixed z-50 grid grid-cols-5 gap-1.5 p-2 rounded-lg bg-brand-main border border-brand-border shadow-2xl"
    style={`left: ${colorPopoverPos.x}px; top: ${colorPopoverPos.y}px;`}
  >
    {#each GENRE_PALETTE_HUES as _, i (i)}
      <button
        type="button"
        onclick={() => { tagsStore.setGroupColor(colorPopoverFor!, i); colorPopoverFor = null; }}
        class="w-5 h-5 rounded-full border-2 {group?.color_index === i ? 'border-brand-text-primary' : 'border-transparent'}"
        style="background-color: {genreColorHsl(i)}"
        aria-label={`${i}`}
      ></button>
    {/each}
  </div>
{/if}

{#if contextMenuTarget}
  <GenreContextMenu
    x={contextMenuTarget.x}
    y={contextMenuTarget.y}
    name={contextMenuTarget.name}
    isRoot={contextMenuTarget.isRoot}
    onRename={() => startRename(contextMenuTarget!.name)}
    onPromote={contextMenuTarget.isRoot ? undefined : () => tagsStore.promoteTag(contextMenuTarget!.name)}
    onDelete={() => { deleteConfirmName = contextMenuTarget!.name; }}
    onClose={() => { contextMenuTarget = null; }}
  />
{/if}

{#if deleteConfirmName}
  <ConfirmDialog
    title={i18n.t("songTags.deleteBtn", {}, "Delete")}
    message={i18n.t(
      "songTags.deleteConfirmMessage",
      { count: 1 },
      `Remove "${deleteConfirmName}" from every song that carries it? This can't be undone.`
    )}
    confirmLabel={i18n.t("songTags.deleteBtn", {}, "Delete")}
    cancelLabel={i18n.t("songTags.cancelBtn", {}, "Cancel")}
    onConfirm={confirmDeleteTag}
    onCancel={() => { deleteConfirmName = null; }}
  />
{/if}
