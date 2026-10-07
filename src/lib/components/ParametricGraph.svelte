<script lang="ts">
  import { tick } from "svelte";
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import type { EqRanges, ParametricBand, ParametricKind, SettingRange } from "../types/equalizer";
  import {
    PLOT_HEIGHT,
    dbToY,
    formatFreq,
    freqToUnit,
    roundFreq,
    unitToFreq,
    yToDb,
  } from "../utils/eqScale";

  interface Props {
    bands: ParametricBand[];
    selected: number;
    ranges: EqRanges;
    /** Whether the EQ is switched on — the curve greys out when it isn't. */
    active: boolean;
    /** Backend-evaluated response of the whole cascade, log-spaced over `ranges.freq`. */
    response: number[];
    /** Backend-evaluated response of the selected band alone, same sampling. */
    bandResponse: number[];
    onselect: (idx: number) => void;
    onchange: (idx: number, band: ParametricBand) => void;
    onadd: (freq: number) => void;
    onremove: (idx: number) => void;
  }

  let { bands, selected, ranges, active, response, bandResponse, onselect, onchange, onadd, onremove }: Props =
    $props();

  let plot: HTMLDivElement | undefined = $state();

  const KIND_STYLES: Record<ParametricKind, { fill: string; border: string; stroke: string; area: string }> = {
    peak: { fill: "bg-brand-accent", border: "border-brand-accent", stroke: "stroke-brand-accent", area: "fill-brand-accent" },
    low_shelf: { fill: "bg-brand-gold", border: "border-brand-gold", stroke: "stroke-brand-gold", area: "fill-brand-gold" },
    high_shelf: { fill: "bg-sky-400", border: "border-sky-400", stroke: "stroke-sky-400", area: "fill-sky-400" },
  };

  const KIND_LABEL_KEYS: Record<ParametricKind, string> = {
    peak: "equalizer.kindPeak",
    low_shelf: "equalizer.kindLowShelf",
    high_shelf: "equalizer.kindHighShelf",
  };

  const clamp = (v: number, r: SettingRange) => Math.max(r.min, Math.min(r.max, v));
  /** Round to `digits` decimals without float noise (0.1 + 0.2 → 0.3). */
  const round = (v: number, digits: number) => Math.round(v * 10 ** digits) / 10 ** digits;
  const clampFreq = (f: number) => clamp(roundFreq(f), ranges.freq);
  const clampGain = (db: number) => clamp(round(db, 1), ranges.gain_db);
  const clampQ = (q: number) => clamp(round(q, 2), ranges.q);

  /** Catmull-Rom spline through the sampled response, in viewBox units. */
  function curvePath(db: number[]): string {
    if (db.length < 2) return "";
    const pts = db.map((v, i) => ({ x: (i / (db.length - 1)) * 100, y: dbToY(v, ranges.gain_db) }));
    let d = `M ${pts[0].x} ${pts[0].y}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = i > 0 ? pts[i - 1] : pts[i];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = i < pts.length - 2 ? pts[i + 2] : p2;
      const cp1x = p1.x + (p2.x - p0.x) / 6;
      const cp1y = p1.y + (p2.y - p0.y) / 6;
      const cp2x = p2.x - (p3.x - p1.x) / 6;
      const cp2y = p2.y - (p3.y - p1.y) / 6;
      d += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
    }
    return d;
  }

  let responsePath = $derived(curvePath(response));
  let bandPath = $derived(curvePath(bandResponse));
  let zeroY = $derived(dbToY(0, ranges.gain_db));
  let bandArea = $derived(bandPath ? `${bandPath} L 100 ${zeroY} L 0 ${zeroY} Z` : "");

  const AXIS_FREQS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
  let axisFreqs = $derived(AXIS_FREQS.filter((f) => f >= ranges.freq.min && f <= ranges.freq.max));
  let decades = $derived(axisFreqs.filter((f) => Math.log10(f) % 1 === 0));
  let gainLines = $derived([ranges.gain_db.max / 2, 0, ranges.gain_db.min / 2]);

  const xPct = (freq: number) => freqToUnit(freq, ranges.freq) * 100;
  const yPct = (db: number) => (dbToY(db, ranges.gain_db) / PLOT_HEIGHT) * 100;

  /** Frequency and gain under a pointer, from the plot's own box. */
  function pointAt(e: MouseEvent): { freq: number; gain: number } | null {
    if (!plot) return null;
    const rect = plot.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    const unit = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const y = ((e.clientY - rect.top) / rect.height) * PLOT_HEIGHT;
    return { freq: clampFreq(unitToFreq(unit, ranges.freq)), gain: clampGain(yToDb(y, ranges.gain_db)) };
  }

  let dragging: number | null = null;

  function onNodePointerDown(e: PointerEvent, idx: number) {
    if (e.button !== 0) return;
    onselect(idx);
    dragging = idx;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    e.preventDefault();
    (e.currentTarget as HTMLElement).focus();
  }

  function onNodePointerMove(e: PointerEvent, idx: number) {
    if (dragging !== idx) return;
    const p = pointAt(e);
    const band = bands[idx];
    if (!p || !band || (p.freq === band.freq && p.gain === band.gain_db)) return;
    onchange(idx, { ...band, freq: p.freq, gain_db: p.gain });
  }

  function onNodePointerUp(e: PointerEvent) {
    dragging = null;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
  }

  /** Wheel over a node scales its Q. Registered non-passive so the panel doesn't scroll. */
  function wheelQ(node: HTMLElement, idx: number) {
    let index = idx;
    const handler = (e: WheelEvent) => {
      const band = bands[index];
      if (!band || e.deltaY === 0) return;
      e.preventDefault();
      onselect(index);
      const q = clampQ(band.q * (e.deltaY < 0 ? 1.1 : 1 / 1.1));
      if (q !== band.q) onchange(index, { ...band, q });
    };
    node.addEventListener("wheel", handler, { passive: false });
    return {
      update(next: number) {
        index = next;
      },
      destroy() {
        node.removeEventListener("wheel", handler);
      },
    };
  }

  async function focusNode(idx: number) {
    await tick();
    plot?.querySelector<HTMLElement>(`[data-node="${idx}"]`)?.focus();
  }

  function onNodeKeyDown(e: KeyboardEvent, idx: number) {
    const band = bands[idx];
    if (!band) return;
    let next: ParametricBand | null = null;
    switch (e.key) {
      case "ArrowLeft":
      case "ArrowRight": {
        const octaves = (e.shiftKey ? 1 / 3 : 1 / 24) * (e.key === "ArrowRight" ? 1 : -1);
        next = { ...band, freq: clampFreq(band.freq * 2 ** octaves) };
        break;
      }
      case "ArrowUp":
      case "ArrowDown": {
        const step = (e.shiftKey ? 1 : 0.1) * (e.key === "ArrowUp" ? 1 : -1);
        next = { ...band, gain_db: clampGain(band.gain_db + step) };
        break;
      }
      case "PageUp":
      case "PageDown": {
        const factor = e.shiftKey ? 1.25 : 1.05;
        next = { ...band, q: clampQ(e.key === "PageUp" ? band.q * factor : band.q / factor) };
        break;
      }
      case "Delete":
      case "Backspace":
        e.preventDefault();
        if (bands.length > ranges.min_bands) {
          onremove(idx);
          focusNode(Math.min(idx, bands.length - 2));
        }
        return;
      default:
        return;
    }
    e.preventDefault();
    onchange(idx, next);
  }

  function onPlotDblClick(e: MouseEvent) {
    if ((e.target as Element).closest("[data-node]")) return;
    if (bands.length >= ranges.max_bands) return;
    const p = pointAt(e);
    if (p) onadd(p.freq);
  }

  function formatGain(db: number): string {
    return `${db > 0 ? "+" : ""}${formatNumber(db, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}`;
  }

  function nodeLabel(band: ParametricBand, idx: number): string {
    return i18n.t("equalizer.nodeAriaLabel", {
      n: idx + 1,
      type: i18n.t(KIND_LABEL_KEYS[band.kind]),
      freq: `${formatNumber(roundFreq(band.freq), { useGrouping: false })} ${i18n.t("units.hz")}`,
      gain: formatGain(band.gain_db),
      q: formatNumber(band.q, { minimumFractionDigits: 2, maximumFractionDigits: 2 }),
    });
  }

  let selectedKind = $derived(bands[selected]?.kind ?? "peak");
</script>

<div class="flex flex-col gap-2">
  <div class="bg-brand-main border border-brand-border rounded-xl p-3 pl-9 pb-6 relative select-none">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="relative h-56 md:h-64" bind:this={plot} ondblclick={onPlotDblClick} data-testid="eq-plot">
      <svg class="absolute inset-0 w-full h-full overflow-visible" viewBox="0 0 100 {PLOT_HEIGHT}" preserveAspectRatio="none" aria-hidden="true">
        {#each decades as f}
          <line x1={xPct(f)} x2={xPct(f)} y1="0" y2={PLOT_HEIGHT} class="stroke-brand-border" stroke-width="1" vector-effect="non-scaling-stroke" />
        {/each}
        {#each gainLines as g}
          <line
            x1="0" x2="100" y1={dbToY(g, ranges.gain_db)} y2={dbToY(g, ranges.gain_db)}
            class="stroke-brand-border" stroke-width="1" vector-effect="non-scaling-stroke"
            stroke-dasharray={g === 0 ? undefined : "3 3"}
          />
        {/each}
        {#if bandArea}
          <path d={bandArea} class="{KIND_STYLES[selectedKind].area} opacity-10" stroke="none" />
          <path
            d={bandPath}
            data-testid="eq-band-response"
            fill="none"
            class="{KIND_STYLES[selectedKind].stroke} opacity-50"
            stroke-width="1"
            vector-effect="non-scaling-stroke"
          />
        {/if}
        {#if responsePath}
          <path
            d={responsePath}
            data-testid="eq-response"
            fill="none"
            class={active ? "stroke-brand-accent" : "stroke-brand-text-secondary opacity-50"}
            stroke-width="2"
            vector-effect="non-scaling-stroke"
          />
        {/if}
      </svg>

      {#each bands as band, idx (idx)}
        {@const style = KIND_STYLES[band.kind]}
        <div
          data-node={idx}
          role="button"
          tabindex="0"
          aria-label={nodeLabel(band, idx)}
          aria-pressed={selected === idx}
          class="absolute w-5 h-5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 flex items-center justify-center text-[9px] font-bold cursor-grab active:cursor-grabbing outline-none touch-none
            {style.border}
            {band.enabled ? `${style.fill} text-brand-main` : 'bg-brand-main text-brand-text-secondary opacity-60'}
            {selected === idx ? 'ring-2 ring-brand-text-primary ring-offset-2 ring-offset-brand-main z-10' : 'focus-visible:ring-2 focus-visible:ring-brand-text-primary'}"
          style="left: {xPct(band.freq)}%; top: {yPct(band.gain_db)}%;"
          onpointerdown={(e) => onNodePointerDown(e, idx)}
          onpointermove={(e) => onNodePointerMove(e, idx)}
          onpointerup={onNodePointerUp}
          onpointercancel={onNodePointerUp}
          onfocus={() => { if (selected !== idx) onselect(idx); }}
          onkeydown={(e) => onNodeKeyDown(e, idx)}
          use:wheelQ={idx}
        >
          {idx + 1}
        </div>
      {/each}
    </div>

    <!-- Axis labels, positioned on the same scales as the plot. -->
    <div class="absolute left-9 right-3 bottom-1 h-4 text-[9px] font-mono text-brand-text-secondary/70 pointer-events-none" aria-hidden="true">
      {#each axisFreqs as f}
        <span class="absolute -translate-x-1/2" style="left: {xPct(f)}%">{formatFreq(f)}</span>
      {/each}
    </div>
    <div class="absolute left-0 w-8 top-3 h-56 md:h-64 text-[9px] font-mono text-brand-text-secondary/70 pointer-events-none" aria-hidden="true">
      {#each [ranges.gain_db.max, 0, ranges.gain_db.min] as g}
        <span class="absolute right-1 -translate-y-1/2" style="top: {yPct(g)}%">{formatGain(g)}</span>
      {/each}
    </div>
  </div>

  <div class="flex items-start justify-between gap-4 flex-wrap px-1">
    <p class="text-xs text-brand-text-secondary text-pretty flex-1 min-w-48">{i18n.t("equalizer.graphHint")}</p>
    <ul class="flex items-center gap-3 text-xs text-brand-text-secondary">
      {#each Object.keys(KIND_STYLES) as ParametricKind[] as kind}
        <li class="flex items-center gap-1.5">
          <span class="w-2.5 h-2.5 rounded-full {KIND_STYLES[kind].fill}" aria-hidden="true"></span>
          {i18n.t(KIND_LABEL_KEYS[kind])}
        </li>
      {/each}
    </ul>
  </div>
</div>
