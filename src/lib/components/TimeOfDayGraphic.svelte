<script lang="ts">
  import type { DaypartBucket } from "../utils/daypart";

  interface Props {
    counts: Record<DaypartBucket, number>;
    max: number;
    labels: Record<DaypartBucket, string>;
  }

  let { counts, max, labels }: Props = $props();

  const BUCKETS: DaypartBucket[] = ["morning", "afternoon", "evening", "latenight"];

  const W = 1000;
  const H = 400;
  const BAND_W = W / 4;
  const HORIZON_Y = 352;
  const GROUND_H = H - HORIZON_Y;

  let reveals = $derived(
    BUCKETS.map((key) => {
      const ratio = max > 0 ? (counts[key] ?? 0) / max : 0;
      return Math.max(0, Math.min(1, ratio)) * H;
    })
  );
</script>

<svg viewBox="0 0 {W} {H}" class="w-full h-auto block rounded-lg" preserveAspectRatio="xMidYMid meet" role="img" aria-label={labels.morning}>
  <defs>
    <linearGradient id="tod-sky-morning" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#aecbdb" />
      <stop offset="55%" stop-color="#d7dfc9" />
      <stop offset="100%" stop-color="#f3dfa3" />
    </linearGradient>
    <linearGradient id="tod-sky-afternoon" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#a9c4d8" />
      <stop offset="50%" stop-color="#e3dcab" />
      <stop offset="100%" stop-color="#f0c96a" />
    </linearGradient>
    <linearGradient id="tod-sky-evening" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#4d4566" />
      <stop offset="40%" stop-color="#8a5f7c" />
      <stop offset="75%" stop-color="#c97a72" />
      <stop offset="100%" stop-color="#e8a95c" />
    </linearGradient>
    <linearGradient id="tod-sky-latenight" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#171b30" />
      <stop offset="55%" stop-color="#232a4a" />
      <stop offset="100%" stop-color="#3a4066" />
    </linearGradient>
    <linearGradient id="tod-ground" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#4b5a3e" />
      <stop offset="100%" stop-color="#333f2b" />
    </linearGradient>
    <linearGradient id="tod-arc" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0%" stop-color="#e8c877" stop-opacity="0.85" />
      <stop offset="50%" stop-color="#f4ecd8" stop-opacity="0.7" />
      <stop offset="100%" stop-color="#cfd7e6" stop-opacity="0.85" />
    </linearGradient>
    <radialGradient id="tod-sun" cx="50%" cy="50%" r="50%">
      <stop offset="0%" stop-color="#fff3d6" />
      <stop offset="100%" stop-color="#e8a54a" />
    </radialGradient>

    <filter id="tod-desaturate" filterUnits="userSpaceOnUse" x="0" y="0" width={W} height={H}>
      <feColorMatrix type="saturate" values="0.06" />
      <feComponentTransfer>
        <feFuncR type="linear" slope="0.5" intercept="0.02" />
        <feFuncG type="linear" slope="0.5" intercept="0.02" />
        <feFuncB type="linear" slope="0.48" intercept="0.03" />
      </feComponentTransfer>
    </filter>

    <mask id="tod-reveal-mask" maskUnits="userSpaceOnUse" x="0" y="0" width={W} height={H}>
      <rect x="0" y="0" width={W} height={H} fill="black" />
      {#each reveals as h, i}
        <rect x={i * BAND_W} y={H - h} width={BAND_W} height={h} fill="white" />
      {/each}
    </mask>

    <mask id="tod-moon-mask" maskUnits="userSpaceOnUse" x="850" y="70" width="80" height="80">
      <circle cx="890" cy="110" r="26" fill="white" />
      <circle cx="900" cy="104" r="24" fill="black" />
    </mask>

    <g id="tod-scene">
      <rect x="0" y="0" width={BAND_W} height={H} fill="url(#tod-sky-morning)" />
      <rect x={BAND_W} y="0" width={BAND_W} height={H} fill="url(#tod-sky-afternoon)" />
      <rect x={BAND_W * 2} y="0" width={BAND_W} height={H} fill="url(#tod-sky-evening)" />
      <rect x={BAND_W * 3} y="0" width={BAND_W} height={H} fill="url(#tod-sky-latenight)" />

      <!-- stars: evening + late night -->
      <g fill="#f8f3e3">
        <circle cx="640" cy="70" r="1.6" />
        <circle cx="700" cy="120" r="1.2" />
        <circle cx="760" cy="60" r="1.4" />
        <circle cx="820" cy="150" r="1.2" />
        <circle cx="940" cy="55" r="1.3" />
        <circle cx="960" cy="200" r="1.2" />
        <circle cx="900" cy="240" r="1.4" />
        <circle cx="850" cy="280" r="1.1" />
        <circle cx="720" cy="220" r="1.2" />
        <circle cx="670" cy="180" r="1.1" />
        <circle cx="990" cy="130" r="1.4" />
      </g>

      <!-- arc across the sky -->
      <path d="M 55 352 Q 500 -55 945 352" fill="none" stroke="url(#tod-arc)" stroke-width="5" />
      <path d="M 85 352 Q 500 -18 915 352" fill="none" stroke="url(#tod-arc)" stroke-width="2" opacity="0.7" />

      <!-- clouds, morning band -->
      <g fill="#fbf6e8" opacity="0.85">
        <ellipse cx="150" cy="200" rx="46" ry="15" />
        <ellipse cx="185" cy="192" rx="30" ry="12" />
        <ellipse cx="95" cy="255" rx="34" ry="11" />
        <ellipse cx="122" cy="248" rx="22" ry="9" />
      </g>

      <!-- sun, morning band -->
      <circle cx="150" cy="330" r="42" fill="url(#tod-sun)" />

      <!-- crescent moon, late night band -->
      <circle cx="890" cy="110" r="26" fill="#f3ecd0" mask="url(#tod-moon-mask)" />

      <!-- ground -->
      <path d="M 0 {HORIZON_Y} Q 125 {HORIZON_Y - 14} 250 {HORIZON_Y - 4} T 500 {HORIZON_Y - 10} T 750 {HORIZON_Y - 2} T {W} {HORIZON_Y - 12} V {H} H 0 Z" fill="url(#tod-ground)" />
    </g>
  </defs>

  <!-- greyscale base: always fully visible -->
  <use href="#tod-scene" filter="url(#tod-desaturate)" />

  <!-- colour reveal: only shows through where the bucket's play count reaches -->
  <g mask="url(#tod-reveal-mask)">
    <use href="#tod-scene" />
  </g>

  <!-- quarter dividers -->
  <g stroke="#000" stroke-opacity="0.12">
    <line x1={BAND_W} y1="0" x2={BAND_W} y2={H} />
    <line x1={BAND_W * 2} y1="0" x2={BAND_W * 2} y2={H} />
    <line x1={BAND_W * 3} y1="0" x2={BAND_W * 3} y2={H} />
  </g>

  <!-- fill line: brackets exactly how far each bucket's colour reaches.
       The outer edges (x=0, x=W) sit exactly on the viewBox boundary, so
       without an inset half the 3px stroke there would render outside the
       viewBox and get clipped — making Morning's left border and Late
       Night's right border look half as thick as the internal dividers. -->
  {#each reveals as h, i}
    {@const y = Math.min(H - 1.5, Math.max(1.5, H - h))}
    {@const xLeft = i === 0 ? 1.5 : i * BAND_W}
    {@const xRight = i === reveals.length - 1 ? W - 1.5 : (i + 1) * BAND_W}
    <polyline
      points="{xLeft},{H} {xLeft},{y} {xRight},{y} {xRight},{H}"
      fill="none"
      stroke="#fff6dc"
      stroke-width="3"
      stroke-linecap="round"
      stroke-linejoin="round"
      opacity="0.95"
    />
  {/each}
</svg>
