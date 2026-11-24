<script lang="ts">
  /**
   * Right-click menu for choosing a visualizer: each preset with a still thumbnail drawn by
   * the preset itself (its mini instance in preview mode, so it matches the playbar slot),
   * and the current one checked. The VU meter adds its calibration: an Auto toggle and, when
   * it is off, a slider for the level that reads 0 VU.
   */
  import { CheckIcon as Check } from "phosphor-svelte";
  import { onMount } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { visualizerStore } from "../stores/visualizer.svelte";
  import { vuMeterSettings } from "../stores/vuMeterSettings.svelte";
  import { prepareCanvas2d } from "../visualizers/canvas";
  import { VU_METER_PRESET, autoReferenceDbfs } from "../visualizers/vuMeter";
  import { VU_REFERENCE_RANGE_DBFS } from "../visualizers/vuMeterEngine";
  import ContextMenu from "./ContextMenu.svelte";

  interface Props {
    x: number;
    y: number;
    onClose: () => void;
  }

  let { x, y, onClose }: Props = $props();

  // Wider than other menus: each row carries a thumbnail as well as the name.
  const MENU_WIDTH_PX = 288;

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

  const manualLevel = $derived(vuMeterSettings.referenceDbfs);
  const autoLevel = $derived(Math.round(autoReferenceDbfs()));
  const sliderFillPercent = $derived(
    manualLevel === null
      ? 0
      : ((manualLevel - VU_REFERENCE_RANGE_DBFS.min) /
          (VU_REFERENCE_RANGE_DBFS.max - VU_REFERENCE_RANGE_DBFS.min)) *
          100,
  );

  /** A level for display, with a true minus sign: "−13 dBFS". */
  function levelText(dbfs: number): string {
    return i18n.t("visualizers.vu.referenceValue", {
      level: String(Math.round(dbfs)).replace("-", "−"),
    });
  }

  // Leaving Auto starts from the level it was using, so the needles don't jump.
  function toggleAuto() {
    if (manualLevel === null) vuMeterSettings.setManual(autoLevel);
    else vuMeterSettings.setAuto();
  }

  function choose(id: string) {
    visualizerStore.select(id);
    onClose();
  }
</script>

<ContextMenu {x} {y} {onClose} widthPx={MENU_WIDTH_PX}>
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
  {#if visualizerStore.preset === VU_METER_PRESET}
    <div class="my-1 border-t border-brand-border" role="separator"></div>
    <div class="label-heading px-3 pt-1 pb-1 text-brand-text-secondary" role="presentation">
      {i18n.t("visualizers.vu.referenceTitle")}
    </div>
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={manualLevel === null}
      data-testid="vu-reference-auto"
      onclick={toggleAuto}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 transition-colors hover:bg-brand-main hover:text-brand-text-primary"
    >
      <span class="flex-1" class:font-semibold={manualLevel === null}
        >{i18n.t("visualizers.vu.referenceAuto")}</span
      >
      {#if manualLevel === null}
        <span class="tabular-nums text-brand-text-secondary">{levelText(autoLevel)}</span>
      {/if}
      <Check
        class="w-3.5 h-3.5 shrink-0 text-brand-accent-text {manualLevel === null
          ? ''
          : 'invisible'}"
        aria-hidden="true"
      />
    </button>
    {#if manualLevel !== null}
      <div class="px-3 pt-1 pb-1.5 flex items-center gap-2.5" role="group">
        <input
          type="range"
          class="vu-slider flex-1 h-1 rounded-lg outline-none"
          data-testid="vu-reference-slider"
          min={VU_REFERENCE_RANGE_DBFS.min}
          max={VU_REFERENCE_RANGE_DBFS.max}
          step="1"
          value={manualLevel}
          oninput={(e) => vuMeterSettings.setManual(parseFloat(e.currentTarget.value))}
          style="background: linear-gradient(to right, var(--color-accent) {sliderFillPercent}%, var(--color-border) {sliderFillPercent}%)"
          aria-label={i18n.t("visualizers.vu.referenceTitle")}
          aria-valuetext={levelText(manualLevel)}
        />
        <span class="w-20 text-right tabular-nums text-brand-text-secondary"
          >{levelText(manualLevel)}</span
        >
      </div>
    {/if}
    <div class="label-small px-3 pb-1 text-brand-text-secondary" role="presentation">
      {i18n.t("visualizers.vu.referenceHint")}
    </div>
  {/if}
</ContextMenu>

<style>
  .vu-slider {
    -webkit-appearance: none;
    appearance: none;
  }
  .vu-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ffffff;
    border: 2px solid var(--color-accent);
  }
  .vu-slider::-moz-range-thumb {
    width: 12px;
    height: 12px;
    border: 2px solid var(--color-accent);
    border-radius: 50%;
    background: #ffffff;
  }
</style>
