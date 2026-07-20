<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let waveformData = $state<number[]>([]);
  let moodbarData = $state<number[]>([]);
  let isDragging = $state(false);

  // Guards a slow, still-in-flight request from a previously-skipped-past
  // track from overwriting waveformData after a newer track has already
  // taken over (e.g. the in-flight request settles just after another skip).
  let waveformRequestId = 0;
  let moodbarRequestId = 0;

  // Fetch waveform when current song changes. get_waveform_data() falls back
  // to a full offline decode of the audio file (decode_all_samples) on a
  // cache miss, which is expensive — rapid-fire skips must not each trigger
  // one, or a burst of skips queues up several concurrent full-file decodes
  // that compete with real-time playback for CPU/disk and can make the
  // whole app feel stuck until they drain. Debounced in the $effect below.
  async function loadWaveform(songId: number | undefined) {
    const requestId = ++waveformRequestId;
    if (songId === undefined) {
      waveformData = [];
      return;
    }
    try {
      const data = await invoke<number[] | null>("get_waveform_data", { songId });
      if (requestId !== waveformRequestId) return; // superseded by a newer track
      if (data) {
        waveformData = data;
      } else {
        // Fallback flat peaks if no waveform exists yet
        waveformData = Array(150).fill(40);
      }
    } catch (e) {
      if (requestId !== waveformRequestId) return;
      console.error("Failed to load waveform:", e);
      waveformData = Array(150).fill(40);
    }
  }

  // Same cache-miss-triggers-full-decode cost as loadWaveform above, so it
  // gets the same request-id guard and debounce treatment.
  async function loadMoodbar(songId: number | undefined) {
    const requestId = ++moodbarRequestId;
    if (songId === undefined) {
      moodbarData = [];
      return;
    }
    try {
      const data = await invoke<number[] | null>("get_moodbar_data", { songId });
      if (requestId !== moodbarRequestId) return;
      moodbarData = data ?? [];
    } catch (e) {
      if (requestId !== moodbarRequestId) return;
      console.error("Failed to load moodbar:", e);
      moodbarData = [];
    }
  }

  // Blends a mood color toward a neutral anchor at the given strength
  // (0 = pure anchor, 1 = pure mood color) — used to keep raw FFT-derived
  // RGB from looking neon/harsh, without fighting the moodbar's own color
  // coding (DESIGN.md: accent is the only interactive-emphasis hue, so
  // moodbar colors are presented as muted content, not competing chrome).
  function blend(mood: [number, number, number], anchor: [number, number, number], strength: number): string {
    const r = Math.round(anchor[0] + (mood[0] - anchor[0]) * strength);
    const g = Math.round(anchor[1] + (mood[1] - anchor[1]) * strength);
    const b = Math.round(anchor[2] + (mood[2] - anchor[2]) * strength);
    return `rgb(${r}, ${g}, ${b})`;
  }

  function hexToRgb(hex: string): [number, number, number] {
    const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})/i.exec(hex);
    return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : [55, 65, 81];
  }

  // Desaturates a color to its luminance-equivalent gray. The border color
  // isn't always neutral — the album-art-adaptive theme derives it from the
  // current track's cover art, so blending moodbar colors toward its raw
  // hue means the theme's color fights the moodbar's own bass/mid/treble
  // color coding. Blending toward this gray keeps the theme's brightness
  // (dark/light contrast) without injecting a competing hue.
  function toGray(rgb: [number, number, number]): [number, number, number] {
    const l = Math.round(0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]);
    return [l, l, l];
  }

  // Draw waveform or moodbar, depending on prefs.seekBarMode
  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;

    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr;
      canvas.height = height * dpr;
      ctx.scale(dpr, dpr);
    }

    ctx.clearRect(0, 0, width, height);

    const songLength = playerStore.currentSong?.length_nanosec || 1;
    const progressPct = playerStore.positionNanosec / songLength;

    // Dynamically read active theme colors from themeStore
    const colors = themeStore.resolvedColors;
    const accentColor = colors["color-accent"] || '#8b5cf6';
    const hoverColor = colors["color-accent-hover"] || '#a78bfa';
    const borderCol = colors["color-border"] || '#374151';

    if (prefs.seekBarMode === "moodbar") {
      drawMoodbar(ctx, width, height, progressPct, accentColor, borderCol);
    } else {
      drawWaveform(ctx, width, height, progressPct, accentColor, hoverColor, borderCol);
    }
  }

  function drawWaveform(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    progressPct: number,
    accentColor: string,
    hoverColor: string,
    borderCol: string,
  ) {
    const data = waveformData.length > 0 ? waveformData : Array(150).fill(40);
    const numBars = data.length;
    const barGap = 1.5;
    const barWidth = (width - (numBars - 1) * barGap) / numBars;

    // Premium gradients for played part
    const gradPlayed = ctx.createLinearGradient(0, height, 0, 0);
    gradPlayed.addColorStop(0, accentColor);
    gradPlayed.addColorStop(1, hoverColor);

    for (let i = 0; i < numBars; i++) {
      const val = data[i] / 255.0;
      // Center the bars vertically
      const barHeight = Math.max(2, val * height * 0.85);
      const x = i * (barWidth + barGap);
      const y = (height - barHeight) / 2;

      const barPct = i / numBars;
      if (barPct <= progressPct) {
        ctx.fillStyle = gradPlayed;
      } else {
        ctx.fillStyle = borderCol;
      }

      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, 1);
      ctx.fill();
    }
  }

  function drawMoodbar(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    progressPct: number,
    accentColor: string,
    borderCol: string,
  ) {
    const totalPoints = moodbarData.length > 0 ? Math.floor(moodbarData.length / 3) : 150;
    const borderRgb = hexToRgb(borderCol);
    const grayAnchor = toGray(borderRgb);

    // Downsample the 150 raw points into broad, averaged regions instead of
    // drawing one bar per point. 150 individually-colored 1-2px bars read
    // as visual noise/a barcode; averaging groups of adjacent points into
    // ~40 wider contiguous blocks reveals the track's actual color
    // structure (verse/chorus-scale regions) the way a real moodbar strip
    // is meant to read at a glance.
    const segmentCount = Math.min(40, totalPoints);
    const groupSize = Math.max(1, Math.ceil(totalPoints / segmentCount));
    const segCount = Math.ceil(totalPoints / groupSize);
    const segWidth = width / segCount;

    for (let s = 0; s < segCount; s++) {
      const start = s * groupSize;
      const end = Math.min(start + groupSize, totalPoints);

      let mood: [number, number, number] = grayAnchor;
      if (moodbarData.length > 0) {
        let r = 0;
        let g = 0;
        let b = 0;
        let n = 0;
        for (let i = start; i < end; i++) {
          r += moodbarData[i * 3];
          g += moodbarData[i * 3 + 1];
          b += moodbarData[i * 3 + 2];
          n++;
        }
        mood = [r / n, g / n, b / n];
      }

      const x = s * segWidth;
      const segPct = s / segCount;
      const played = segPct <= progressPct;

      // Unplayed: desaturated/low-opacity, blended toward a neutral gray
      // (not the theme's raw border hue — see toGray() above). Played: full
      // mood color, with a thin accent-colored cap on top so the accent hue
      // still carries the "this is progress" signal instead of the mood
      // colors alone (accent stays the only interactive-emphasis hue per
      // DESIGN.md). Segments are drawn contiguous (a hair of overlap, no
      // gap) so adjacent same-colored regions blend into one visual block
      // rather than a striped bar.
      ctx.fillStyle = blend(mood, grayAnchor, played ? 0.85 : 0.5);
      ctx.fillRect(x, 0, segWidth + 0.5, height);

      if (played) {
        ctx.fillStyle = accentColor;
        ctx.fillRect(x, 0, segWidth + 0.5, 1.5);
      }
    }
  }

  // React to changes in currentSong (or a mode toggle) using Svelte 5
  // $effect. Debounced: the cleanup callback cancels the pending timer
  // whenever songId/mode changes again before it fires, so a burst of rapid
  // skips only ever loads data for whichever track the user actually
  // settles on. Only the active mode's data is fetched — switching modes
  // lazily loads the other strip rather than always fetching both.
  $effect(() => {
    const songId = playerStore.currentSong?.id;
    const mode = prefs.seekBarMode;
    const timer = setTimeout(() => {
      if (mode === "moodbar") {
        loadMoodbar(songId);
      } else {
        loadWaveform(songId);
      }
    }, 300);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    // Redraw whenever position, length, theme, artwork colors, mode, or data updates
    const _pos = playerStore.positionNanosec;
    const _len = playerStore.currentSong?.length_nanosec;
    const _theme = themeStore.activeThemeId;
    const _art = themeStore.artworkColors;
    const _mode = prefs.seekBarMode;
    const _wave = waveformData;
    const _mood = moodbarData;
    draw();
  });

  // Handle seek actions (click / drag)
  function seekToX(clientX: number) {
    if (!canvas || !playerStore.currentSong) return;
    const rect = canvas.getBoundingClientRect();
    const x = Math.max(0, Math.min(clientX - rect.left, rect.width));
    const pct = x / rect.width;
    const targetNs = pct * (playerStore.currentSong.length_nanosec || 0);
    playerStore.seek(targetNs);
  }

  function handleMouseDown(e: MouseEvent) {
    if (!playerStore.currentSong) return;
    isDragging = true;
    seekToX(e.clientX);
  }

  function handleMouseMove(e: MouseEvent) {
    if (isDragging) {
      seekToX(e.clientX);
    }
  }

  function handleMouseUp() {
    isDragging = false;
  }
</script>

<svelte:window onmouseup={handleMouseUp} onmousemove={handleMouseMove} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  onmousedown={handleMouseDown}
  class="relative flex-1 h-8 cursor-pointer flex items-center group select-none"
  title={prefs.seekBarMode === 'moodbar'
    ? i18n.t('playerBar.moodbarLegend', {}, 'Moodbar — color reflects the track\'s frequency balance: red = bass, green = mids, blue = treble; brighter regions carry more energy in that band')
    : undefined}
>
  <canvas bind:this={canvas} class="w-full h-7 opacity-100"></canvas>
</div>
