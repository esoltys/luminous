<script lang="ts">
  import { DotsSixVerticalIcon as GripVertical } from "phosphor-svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import {
    genreColorHsl,
    genreColorHslBright,
    genreColorHslDark,
    getGenreColorChoices,
  } from "../utils/genrePalette";
  import { portal } from "../utils/portal";
  import { startPointerDrag, type PointerDragHandle } from "../utils/pointerDrag";
  import { themeStore } from "../stores/theme.svelte";
  import { isLightColor } from "../utils/colorUtils";
  import GenreContextMenu from "./GenreContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import ColorPicker from "./ColorPicker.svelte";
  import DragGhost from "./DragGhost.svelte";
  import { cardDropApplies, chipDropAction, type TagCardDropTarget } from "../utils/tagCardDrop";

  const genreColorChoices = getGenreColorChoices();

  /** Subgenre chip text/border must read clearly against the chip's own pale
   * fill, which tracks the active theme: bright text only works on a dark
   * theme's near-black fill, so a light theme needs the dark/saturated
   * variant instead. */
  let isLightTheme = $derived(isLightColor(themeStore.resolvedColors["bg-main"]));
  let genreColorHslFg = $derived(isLightTheme ? genreColorHslDark : genreColorHslBright);

  interface Props {
    onOpenMainTag: (tag: string) => void;
    onOpenGenreEdge: (root: string, child: string) => void;
    sortField?: "name" | "count";
    sortAsc?: boolean;
    /** Collapses cards down to compact header-only rows (mirrors the
     * Albums/Artists cards-vs-rows toggle). */
    compact?: boolean;
    /** Songs with no genre value at all — rendered as a trailing card in the
     * same grid, matching the genre cards' size/styling, rather than as a
     * separate full-width element below the grid. */
    noGenreCount?: number;
    onOpenNoGenre?: () => void;
  }

  let {
    onOpenMainTag,
    onOpenGenreEdge,
    sortField = "name",
    sortAsc = true,
    compact = false,
    noGenreCount = 0,
    onOpenNoGenre,
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

  let contextMenuTarget = $state<{ x: number; y: number; name: string; isRoot: boolean } | null>(
    null,
  );
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
      i18n.t(
        "songTags.renameToast",
        { count, name: into },
        `Renamed to "${into}" (${count} songs updated)`,
      ),
      "success",
    );
  }

  async function confirmDeleteTag() {
    const name = deleteConfirmName;
    deleteConfirmName = null;
    if (!name) return;
    const count = await tagsStore.deleteTags([name]);
    toastStore.show(i18n.plural("songTags.deleteToast", count), "success");
  }

  /** Sorts both the cards themselves and each card's own children — display
   * only, doesn't touch the persisted drag-reorder sort_order. */
  let sortedHierarchy = $derived.by(() => {
    const dir = sortAsc ? 1 : -1;
    const cmp = (
      a: { name: string; song_count: number },
      b: { name: string; song_count: number },
    ) =>
      sortField === "name"
        ? a.name.localeCompare(b.name) * dir
        : (a.song_count - b.song_count) * dir;
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
  let dropTarget = $state<TagCardDropTarget | null>(null);
  /** Cursor position while a drag is active, for the floating ghost label —
   * cleared alongside draggedChip/draggedCard on pointerup. */
  let pointerPos = $state<{ x: number; y: number } | null>(null);
  /** A chip's whole surface (not just its edge padding) needs to be grabbable, so click-to-open
   * can't be a separate <button> the drag simply avoids; instead the shared pointer-drag helper's
   * threshold decides whether the gesture was a click (`onTap`) or a drag. */
  let tagDrag: PointerDragHandle | null = null;
  /** Reactive so the ghost only appears once the drag has armed — a plain click
   * on a chip must not flash it. */
  let dragMoved = $state(false);

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

  $effect(() => () => tagDrag?.cancel());

  function endDrag() {
    draggedChip = null;
    draggedCard = null;
    dropTarget = null;
    pointerPos = null;
    dragMoved = false;
    tagDrag = null;
  }

  /** Tracks one press-drag-release; `grab` records what was picked up once the press is accepted. */
  function beginDrag(e: PointerEvent, ignoreSelector: string | undefined, grab: () => void) {
    tagDrag = startPointerDrag(e, {
      // Capture only once a drag is real: capture on press would retarget a plain click.
      captureOn: "arm",
      ignoreSelector,
      onArm: () => {
        dragMoved = true;
      },
      onMove: handlePointerMove,
      onDrop: handleDrop,
      onTap: handleTap,
      onCancel: endDrag,
    });
    if (!tagDrag) return;
    // Right-click (and middle-click) never reach here: calling preventDefault on any
    // pointerdown suppresses the compatibility mouse events it's derived from, which was
    // silently swallowing the right-button's contextmenu trigger too.
    e.preventDefault();
    grab();
    pointerPos = { x: e.clientX, y: e.clientY };
  }

  function handleChipPointerDown(e: PointerEvent, name: string, fromGroup: string) {
    // The rename input and the a11y checkbox handle their own clicks — don't hijack them into a drag.
    beginDrag(e, "input", () => (draggedChip = { name, fromGroup }));
  }

  function handleCardPointerDown(e: PointerEvent, name: string) {
    beginDrag(e, undefined, () => (draggedCard = name));
  }

  function handlePointerMove(pos: { x: number; y: number }) {
    pointerPos = pos;
    const el = document.elementFromPoint(pos.x, pos.y) as HTMLElement | null;
    const chipEl = el?.closest<HTMLElement>("[data-chip-key]");
    const headerEl = el?.closest<HTMLElement>("[data-card-header]");
    const cardEl = el?.closest<HTMLElement>("[data-card-name]");
    if (chipEl) {
      dropTarget = {
        kind: "chip",
        group: chipEl.dataset.chipGroup!,
        chip: chipEl.dataset.chipKey!,
      };
    } else if (headerEl) {
      dropTarget = { kind: "header", group: headerEl.dataset.cardHeader! };
    } else if (cardEl) {
      dropTarget = { kind: "card", group: cardEl.dataset.cardName! };
    } else {
      dropTarget = null;
    }
  }

  /** Clicking anywhere on a card other than a sub-genre chip (or one of the
   * header's own controls) opens the card's drill-down — matches the
   * inner name/count button's action, just extended to the whole card so
   * users don't have to aim for that one small area. */
  function handleCardClick(e: MouseEvent, name: string) {
    const target = e.target as HTMLElement;
    if (target.closest("[data-chip-key], [data-color-swatch-for]")) return;
    if (target.tagName === "INPUT") return;
    onOpenMainTag(name);
  }

  let dropApplies = $derived(
    draggedCard
      ? cardDropApplies(draggedCard, dropTarget)
      : draggedChip
        ? chipDropAction(draggedChip, dropTarget) !== null
        : false,
  );

  // Barely moved — treat the whole gesture as a plain click on the chip rather than a drag, so
  // the entire pill (not just its edge) opens the drill-down view.
  function handleTap() {
    const chip = draggedChip;
    endDrag();
    if (chip) onOpenGenreEdge(chip.fromGroup, chip.name);
  }

  async function handleDrop() {
    const chip = draggedChip;
    const card = draggedCard;
    const target = dropTarget;
    endDrag();

    if (card) {
      if (cardDropApplies(card, target)) {
        await tagsStore.demoteGroupToChild(card, target!.group);
      }
      return;
    }

    if (!chip) return;

    switch (chipDropAction(chip, target)) {
      case "promote":
        await tagsStore.promoteTag(chip.name);
        break;
      case "reparent":
        await tagsStore.reparentTag(chip.name, target!.group);
        break;
      case "reorder": {
        const group = tagsStore.hierarchy.find((g) => g.name === chip.fromGroup);
        const newIndex = group?.children.findIndex((c) => c.name === target!.chip) ?? 0;
        await tagsStore.reorderTagInGroup(chip.name, newIndex);
        break;
      }
    }
  }
</script>

<svelte:window onmousedown={handleWindowMouseDown} />

<div class="grid grid-cols-1 @xl:grid-cols-2 @5xl:grid-cols-3 gap-3">
  {#each sortedHierarchy as group (group.name)}
    {@const cardHighlighted =
      (dropTarget?.kind === "card" || dropTarget?.kind === "header") &&
      dropTarget.group === group.name &&
      (draggedChip || draggedCard)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      data-card-name={group.name}
      onclick={(e) => handleCardClick(e, group.name)}
      class="rounded-lg bg-brand-sidebar border-2 overflow-hidden transition-[opacity,box-shadow,border-color,transform] cursor-pointer {draggedCard ===
      group.name
        ? 'opacity-40'
        : ''} {cardHighlighted
        ? 'border-brand-accent ring-4 ring-brand-accent/50 scale-[1.02] bg-brand-accent/5'
        : ''}"
      style={cardHighlighted ? "" : `border-color: ${genreColorHsl(group.color_index)}`}
    >
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        data-card-header={group.name}
        oncontextmenu={(e) => openContextMenu(e, group.name, true)}
        class="flex items-center gap-2 px-3 py-2.5 transition-colors {dropTarget?.kind ===
          'header' && dropTarget.group === group.name
          ? 'bg-brand-accent/25'
          : ''}"
      >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          onpointerdown={(e) => handleCardPointerDown(e, group.name)}
          class="shrink-0 touch-none genre-drag-handle {draggedCard === group.name
            ? 'is-dragging'
            : ''} text-brand-text-secondary/50 hover:text-brand-text-secondary"
          title={i18n.t(
            "songTags.dragCardTooltip",
            {},
            "Drag to make this a sub-genre of another card",
          )}
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
              {i18n.plural("songTags.songCount", group.song_count)}
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
            oncontextmenu={(e) => openContextMenu(e, child.name, false)}
            class="inline-flex items-center gap-1 pl-2 pr-1.5 py-0.5 rounded-full border-2 bg-[color-mix(in_srgb,var(--color-brand-accent)_15%,var(--color-brand-sidebar))] text-brand-text-primary text-xs font-medium select-none touch-none transition-[opacity,box-shadow,transform] genre-drag-handle {draggedChip?.name ===
            child.name
              ? 'is-dragging opacity-40'
              : ''} {dropTarget?.kind === 'chip' && dropTarget.chip === child.name
              ? 'ring-4 ring-brand-accent scale-110'
              : ''}"
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
              <!-- Click-to-open is handled by handlePointerUp (a chip
                   pointerdown/up with no meaningful movement) so the whole
                   pill is grabbable for dragging, not just a <button>'s
                   edge padding around the label. -->
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
  {#if noGenreCount > 0}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      onclick={onOpenNoGenre}
      class="rounded-lg bg-brand-sidebar border-2 border-brand-border/60 hover:border-brand-accent/60 overflow-hidden transition-colors cursor-pointer"
    >
      <div class="flex items-center gap-2 px-3 py-2.5">
        <span class="flex-1 min-w-0 flex items-center justify-between gap-2">
          <span class="text-sm font-semibold text-brand-text-primary truncate">
            {i18n.t("songTags.noGenre", {}, "No Genre")}
          </span>
          <span class="text-xs text-brand-text-secondary tabular-nums shrink-0">
            {i18n.plural("songTags.songCount", noGenreCount)}
          </span>
        </span>
      </div>
      <div class="px-3 pb-3 min-h-9 {compact ? 'hidden' : ''}"></div>
    </div>
  {/if}
</div>

{#if ghostInfo && pointerPos && dragMoved}
  <DragGhost
    x={pointerPos.x}
    y={pointerPos.y}
    overTarget={dropApplies}
    color={genreColorHsl(ghostInfo.colorIndex)}
    testId="genre-drag-ghost"
  >
    <span class="max-w-[180px] truncate font-semibold">{ghostInfo.label}</span>
  </DragGhost>
{/if}

{#if colorPopoverFor && colorPopoverPos}
  {@const group = sortedHierarchy.find((g) => g.name === colorPopoverFor)}
  <div
    use:portal
    bind:this={colorPopoverEl}
    class="fixed z-50 p-2 rounded-lg bg-brand-main border border-brand-border shadow-2xl"
    style={`left: ${colorPopoverPos.x}px; top: ${colorPopoverPos.y}px;`}
  >
    <ColorPicker
      choices={genreColorChoices}
      value={group ? String(group.color_index) : null}
      onChange={(v) => {
        tagsStore.setGroupColor(colorPopoverFor!, Number(v));
        colorPopoverFor = null;
      }}
      size="sm"
      columns={5}
    />
  </div>
{/if}

{#if contextMenuTarget}
  <GenreContextMenu
    x={contextMenuTarget.x}
    y={contextMenuTarget.y}
    name={contextMenuTarget.name}
    isRoot={contextMenuTarget.isRoot}
    onRename={() => startRename(contextMenuTarget!.name)}
    onPromote={contextMenuTarget.isRoot
      ? undefined
      : () => tagsStore.promoteTag(contextMenuTarget!.name)}
    onAlias={() => tagsStore.openAliasDialog(contextMenuTarget!.name)}
    onDelete={() => {
      deleteConfirmName = contextMenuTarget!.name;
    }}
    onClose={() => {
      contextMenuTarget = null;
    }}
  />
{/if}

{#if deleteConfirmName}
  <ConfirmDialog
    title={i18n.t("songTags.deleteBtn", {}, "Delete")}
    message={i18n.plural("songTags.deleteConfirmMessage", 1)}
    confirmLabel={i18n.t("songTags.deleteBtn", {}, "Delete")}
    cancelLabel={i18n.t("songTags.cancelBtn", {}, "Cancel")}
    onConfirm={confirmDeleteTag}
    onCancel={() => {
      deleteConfirmName = null;
    }}
  />
{/if}

<style>
  /* app.css resets cursor to default everywhere for this desktop app, and
     `cursor` doesn't reliably inherit into a phosphor-svelte icon's rendered
     <svg> from its wrapping element (confirmed via devtools: the <svg>'s
     own computed cursor stayed "default" even though its parent <span>
     correctly computed "grab") — so these drag handles set cursor
     explicitly on every descendant rather than relying on inheritance.
     :not(input) excludes the inline chip-rename <input>, which keeps its
     own text-caret cursor from app.css. */
  .genre-drag-handle,
  .genre-drag-handle :global(*:not(input)) {
    cursor: grab !important;
  }
  .genre-drag-handle.is-dragging,
  .genre-drag-handle.is-dragging :global(*:not(input)) {
    cursor: grabbing !important;
  }
</style>
