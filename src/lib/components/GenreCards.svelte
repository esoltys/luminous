<script lang="ts">
  import { AlertTriangle } from "lucide-svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { GENRE_PALETTE_HUES, genreColorHsl } from "../utils/genrePalette";

  interface Props {
    onOpenMainTag: (tag: string) => void;
    onOpenGenreEdge: (root: string, child: string) => void;
    selectMode: boolean;
    selected: Set<string>;
    onToggleSelect: (name: string) => void;
  }

  let { onOpenMainTag, onOpenGenreEdge, selectMode, selected, onToggleSelect }: Props = $props();

  let colorPopoverFor = $state<string | null>(null);

  // Drag reparent/promote/reorder cannot use native HTML5 drag-and-drop —
  // Tauri intercepts OS-level drag-drop at the webview layer so `dragstart`
  // never fires in-page (see ChipInput.svelte). Mirrors its pointer-event
  // pattern instead.
  let draggedChip = $state<{ name: string; fromGroup: string } | null>(null);
  let dropTarget = $state<{ kind: "card" | "header" | "chip"; group: string; chip?: string } | null>(null);

  function handleChipPointerDown(e: PointerEvent, name: string, fromGroup: string) {
    if (selectMode) return;
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    draggedChip = { name, fromGroup };
  }

  function handlePointerMove(e: PointerEvent) {
    if (!draggedChip) return;
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
    const target = dropTarget;
    draggedChip = null;
    dropTarget = null;
    if (!chip || !target) return;

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

<svelte:window onpointermove={handlePointerMove} onpointerup={handlePointerUp} />

<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
  {#each tagsStore.hierarchy as group (group.name)}
    <div
      data-card-name={group.name}
      class="rounded-lg bg-brand-sidebar border overflow-hidden transition-colors {dropTarget?.kind === 'card' && dropTarget.group === group.name ? 'border-brand-accent ring-2 ring-brand-accent/40' : 'border-brand-border/60'}"
    >
      <div
        data-card-header={group.name}
        class="flex items-center gap-2 px-3 py-2.5 transition-colors {dropTarget?.kind === 'header' && dropTarget.group === group.name ? 'bg-brand-accent/15' : ''}"
      >
        <div class="relative shrink-0">
          <button
            type="button"
            onclick={() => { colorPopoverFor = colorPopoverFor === group.name ? null : group.name; }}
            class="w-4 h-4 rounded-full border border-black/10 shrink-0"
            style="background-color: {genreColorHsl(group.color_index)}"
            title={i18n.t("songTags.changeColorTooltip", {}, "Change color")}
            aria-label={i18n.t("songTags.changeColorTooltip", {}, "Change color")}
          ></button>
          {#if colorPopoverFor === group.name}
            <div class="absolute z-20 top-6 left-0 grid grid-cols-5 gap-1.5 p-2 rounded-lg bg-brand-main border border-brand-border shadow-2xl">
              {#each GENRE_PALETTE_HUES as _, i (i)}
                <button
                  type="button"
                  onclick={() => { tagsStore.setGroupColor(group.name, i); colorPopoverFor = null; }}
                  class="w-5 h-5 rounded-full border-2 {group.color_index === i ? 'border-brand-text-primary' : 'border-transparent'}"
                  style="background-color: {genreColorHsl(i)}"
                  aria-label={`${i}`}
                ></button>
              {/each}
            </div>
          {/if}
        </div>
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
      </div>

      <div class="px-3 pb-3 flex flex-wrap gap-1.5 min-h-9">
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
            class="inline-flex items-center gap-1 pl-2 pr-1.5 py-1 rounded-full border text-xs font-medium select-none touch-none transition-[opacity,box-shadow] {selectMode ? 'cursor-pointer' : 'cursor-grab active:cursor-grabbing'} {draggedChip?.name === child.name ? 'opacity-40' : ''} {(dropTarget?.kind === 'chip' && dropTarget.chip === child.name) || selected.has(child.name) ? 'ring-2 ring-brand-accent' : ''}"
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
            <button
              type="button"
              onclick={() => !selectMode && onOpenGenreEdge(group.name, child.name)}
              class="inline-flex items-baseline gap-1"
            >
              <span>{child.name}</span>
              <span class="text-[0.85em] opacity-70">{child.song_count}</span>
            </button>
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
