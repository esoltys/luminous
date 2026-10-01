<script lang="ts">
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { PlusIcon as Plus, XIcon as X } from "phosphor-svelte";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import type { EqRanges, ParametricBand, ParametricKind } from "../types/equalizer";
  import { widestGapFreq } from "../utils/eqScale";

  interface Props {
    bands: ParametricBand[];
    selected: number;
    ranges: EqRanges;
    onselect: (idx: number) => void;
    /** Resolves once the backend has echoed the canonical config. */
    onchange: (idx: number, band: ParametricBand) => Promise<void> | void;
    onadd: (freq: number) => void;
    onremove: (idx: number) => void;
  }

  let { bands, selected, ranges, onselect, onchange, onadd, onremove }: Props = $props();

  const KINDS: { kind: ParametricKind; key: string }[] = [
    { kind: "peak", key: "equalizer.kindPeak" },
    { kind: "low_shelf", key: "equalizer.kindLowShelf" },
    { kind: "high_shelf", key: "equalizer.kindHighShelf" },
  ];

  type NumericField = "freq" | "gain_db" | "q";

  /** Decimals shown per field — the engine stores f32, so Q 1/√2 arrives as 0.70710677. */
  const DIGITS: Record<NumericField, number> = { freq: 1, gain_db: 1, q: 2 };
  const show = (band: ParametricBand, field: NumericField) => {
    const val = Math.round(band[field] * 10 ** DIGITS[field]) / 10 ** DIGITS[field];
    return formatNumber(val, { minimumFractionDigits: 0, maximumFractionDigits: DIGITS[field] });
  };

  function parseLocalized(str: string): number {
    const normalized = str.trim().replace(/\s/g, "").replace(",", ".");
    return parseFloat(normalized);
  }

  /** Commit a typed value, then show whatever the backend echoed — the input
   * never keeps a value the engine clamped away. */
  async function commitNumber(e: Event & { currentTarget: HTMLInputElement }, idx: number, field: NumericField) {
    const input = e.currentTarget;
    const band = bands[idx];
    if (!band) return;
    const value = parseLocalized(input.value);
    if (Number.isFinite(value) && value !== band[field]) {
      onselect(idx);
      await onchange(idx, { ...band, [field]: value });
    }
    const echoed = bands[idx];
    if (echoed) input.value = show(echoed, field);
  }

  function handleKeyDown(e: KeyboardEvent & { currentTarget: HTMLInputElement }, idx: number, field: NumericField, step: number) {
    if (e.key === "Enter") {
      e.currentTarget.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const currentVal = parseLocalized(e.currentTarget.value);
      if (Number.isFinite(currentVal)) {
        const delta = e.key === "ArrowUp" ? step : -step;
        const range = ranges[field === "gain_db" ? "gain_db" : field];
        const next = Math.round((currentVal + delta) * 100) / 100;
        const clamped = Math.max(range.min, Math.min(range.max, next));
        e.currentTarget.value = show({ ...bands[idx], [field]: clamped }, field);
        e.currentTarget.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }
  }

  let list: HTMLDivElement | undefined = $state();

  // Keep the selected row visible when selection comes from the graph.
  $effect(() => {
    const row = list?.querySelector<HTMLElement>(`[data-row="${selected}"]`);
    row?.scrollIntoView?.({ block: "nearest" });
  });

  const inputClass =
    "w-full min-w-0 bg-brand-main border border-brand-border rounded px-2 py-1 text-xs font-mono text-brand-text-primary outline-none focus:border-brand-accent";
</script>

<div class="flex flex-col gap-2">
  <div class="grid grid-cols-[1.5rem_minmax(6rem,1.4fr)_repeat(3,minmax(4rem,1fr))_auto_auto] gap-2 px-2 text-[10px] font-bold uppercase tracking-wider text-brand-text-secondary" aria-hidden="true">
    <span>#</span>
    <span>{i18n.t("equalizer.bandType")}</span>
    <span>{i18n.t("equalizer.frequency")} (Hz)</span>
    <span>{i18n.t("equalizer.gain")} (dB)</span>
    <span>{i18n.t("equalizer.qFactor")}</span>
    <span class="w-9"></span>
    <span class="w-7"></span>
  </div>

  <div class="max-h-72 overflow-y-auto flex flex-col gap-1" bind:this={list}>
    {#each bands as band, idx (idx)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        data-row={idx}
        class="grid grid-cols-[1.5rem_minmax(6rem,1.4fr)_repeat(3,minmax(4rem,1fr))_auto_auto] gap-2 items-center rounded-lg px-2 py-1.5 border
          {selected === idx ? 'bg-brand-accent/10 border-brand-accent/50' : 'border-transparent hover:bg-brand-main/50'}
          {band.enabled ? '' : 'opacity-60'}"
        onclick={() => { if (selected !== idx) onselect(idx); }}
        onfocusin={() => { if (selected !== idx) onselect(idx); }}
      >
        <span class="text-xs font-bold text-brand-text-secondary">{idx + 1}</span>
        <Select
          value={band.kind}
          onchange={(e) => onchange(idx, { ...band, kind: e.currentTarget.value as ParametricKind })}
          class="w-full bg-brand-main text-xs text-brand-text-primary border border-brand-border rounded pl-2 pr-6 py-1 outline-none focus:border-brand-accent"
          chevronPosition="0.375rem"
        >
          {#each KINDS as k}
            <option value={k.kind} class="bg-brand-main text-brand-text-primary">{i18n.t(k.key)}</option>
          {/each}
        </Select>
        <input
          type="text"
          inputmode="decimal"
          aria-label={`${i18n.t("equalizer.frequency")} ${idx + 1}`}
          min={ranges.freq.min}
          max={ranges.freq.max}
          step="any"
          value={show(band, "freq")}
          onchange={(e) => commitNumber(e, idx, "freq")}
          onkeydown={(e) => handleKeyDown(e, idx, "freq", 10)}
          class={inputClass}
        />
        <input
          type="text"
          inputmode="decimal"
          aria-label={`${i18n.t("equalizer.gain")} ${idx + 1}`}
          min={ranges.gain_db.min}
          max={ranges.gain_db.max}
          step="0.1"
          value={show(band, "gain_db")}
          onchange={(e) => commitNumber(e, idx, "gain_db")}
          onkeydown={(e) => handleKeyDown(e, idx, "gain_db", 0.5)}
          class={inputClass}
        />
        <input
          type="text"
          inputmode="decimal"
          aria-label={`${i18n.t("equalizer.qFactor")} ${idx + 1}`}
          min={ranges.q.min}
          max={ranges.q.max}
          step="0.01"
          value={show(band, "q")}
          onchange={(e) => commitNumber(e, idx, "q")}
          onkeydown={(e) => handleKeyDown(e, idx, "q", 0.05)}
          class={inputClass}
        />
        <Toggle
          checked={band.enabled}
          onchange={(v) => onchange(idx, { ...band, enabled: v })}
          label={i18n.t("equalizer.enableBand", { n: idx + 1 })}
          showOnOffLabel={false}
        />
        <button
          type="button"
          class="w-7 h-7 flex items-center justify-center rounded-full text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main disabled:opacity-30 disabled:pointer-events-none"
          aria-label={i18n.t("equalizer.removeBand", { n: idx + 1 })}
          title={i18n.t("equalizer.removeBand", { n: idx + 1 })}
          disabled={bands.length <= ranges.min_bands}
          onclick={(e) => { e.stopPropagation(); onremove(idx); }}
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    {/each}
  </div>

  <button
    type="button"
    class="self-start flex items-center gap-1.5 text-xs font-semibold px-4 py-1.5 bg-brand-main border border-brand-border rounded-full text-brand-text-secondary hover:text-brand-text-primary disabled:opacity-40 disabled:pointer-events-none"
    disabled={bands.length >= ranges.max_bands}
    onclick={() => onadd(widestGapFreq(bands.map((b) => b.freq), ranges.freq))}
  >
    <Plus class="w-3.5 h-3.5" />
    {i18n.t("equalizer.addBand")}
  </button>
</div>
