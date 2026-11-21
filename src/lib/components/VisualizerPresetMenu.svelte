<script lang="ts">
  /**
   * Right-click menu for choosing a visualizer: each preset with a still thumbnail drawn by
   * the preset itself (its mini instance in preview mode, so it matches the playbar slot),
   * and the current one checked.
   */
  import { CheckIcon as Check } from "phosphor-svelte";
  import { onMount } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { visualizerStore } from "../stores/visualizer.svelte";
  import { prepareCanvas2d } from "../visualizers/canvas";
  import ContextMenu from "./ContextMenu.svelte";

  interface Props {
    x: number;
    y: number;
    onClose: () => void;
  }

  let { x, y, onClose }: Props = $props();

  const thumbnails: (HTMLCanvasElement | null)[] = $state([]);

  // Drawn once when the menu opens; thumbnails are still images, so nothing animates here.
  onMount(() => {
    const colors = themeStore.resolvedColors;
    visualizerStore.presets.forEach((preset, i) => {
      const target = prepareCanvas2d(thumbnails[i] ?? null);
      if (!target) return;
      const instance = preset.create("mini");
      instance.setTheme(colors);
      instance.render(target.ctx, target.width, target.height, {
        reducedMotion: true,
        preview: true,
      });
    });
  });

  function choose(id: string) {
    visualizerStore.select(id);
    onClose();
  }
</script>

<ContextMenu {x} {y} {onClose}>
  <div class="label-heading px-3 pt-0.5 pb-1.5 text-brand-text-secondary" role="presentation">
    {i18n.t("visualizers.menuTitle", {}, "Visualizer")}
  </div>
  {#each visualizerStore.presets as preset, i (preset.id)}
    {@const selected = preset.id === visualizerStore.preset.id}
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      data-testid="visualizer-preset-option"
      data-preset={preset.id}
      onclick={() => choose(preset.id)}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 transition-colors hover:bg-brand-main hover:text-brand-text-primary"
    >
      <canvas
        bind:this={thumbnails[i]}
        aria-hidden="true"
        class="w-16 h-5 shrink-0 rounded-sm bg-brand-sidebar/60"
      ></canvas>
      <span class="flex-1 truncate" class:font-semibold={selected}>{i18n.t(preset.labelKey)}</span>
      <Check
        class="w-3.5 h-3.5 shrink-0 text-brand-accent-text {selected ? '' : 'invisible'}"
        aria-hidden="true"
      />
    </button>
  {/each}
</ContextMenu>
