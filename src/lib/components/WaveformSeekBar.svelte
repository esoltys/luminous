<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";

  const CANVAS_BAR_HEIGHT_PX = 28;
  const NARROW_WIDTH_BREAKPOINT_PX = 450;
  const MOODBAR_SEGMENT_COUNT = 40;

  // Fixed layer colors for the low/mid/high band waveform (moodbar mode). These
  // are a deliberate exception to DESIGN.md's "accent is the only
  // interactive-emphasis hue" rule — like the mood colors they replace, they
  // encode frequency-band data rather than interactive state, so they're kept
  // visually distinct from accentColor (the progress cap below still uses
  // accentColor so "this is progress" stays legible against these bands).
  const LOW_BAND_COLOR = "#3b82f6"; // blue — bass/sub-bass
  const MID_BAND_COLOR = "#f59e0b"; // amber — vocals/snares/mid instruments
  const HIGH_BAND_COLOR = "#f9fafb"; // white — hi-hats/cymbals/transients

  let containerEl = $state<HTMLDivElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let waveformData = $state<number[]>([]);
  let moodbarData = $state<number[]>([]);
  let isDragging = $state(false);

  // Guards a slow, still-in-flight request from a previously-skipped-past
  // track from overwriting waveformData after a newer track has already
  // taken over (e.g. the in-flight request settles just after another skip).
  let waveformRequestId = 0;
  let moodbarRequestId = 0;

  let isLoadingWaveform = $state(false);
  let isLoadingMoodbar = $state(false);
  let pulseAngle = $state(0);
  let animFrameId: number | null = null;

  function startLoadingAnimation() {
    if (animFrameId !== null) return;
    function step() {
      if (isLoadingWaveform || isLoadingMoodbar) {
        pulseAngle = (pulseAngle + 0.08) % (Math.PI * 2);
        draw();
        animFrameId = requestAnimationFrame(step);
      } else {
        if (animFrameId !== null) {
          cancelAnimationFrame(animFrameId);
          animFrameId = null;
        }
        draw();
      }
    }
    animFrameId = requestAnimationFrame(step);
  }

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
      isLoadingWaveform = false;
      draw();
      return;
    }
    isLoadingWaveform = true;
    startLoadingAnimation();
    try {
      const data = await invoke<number[] | null>("get_waveform_data", { song_id: songId, songId });
      if (requestId !== waveformRequestId) return; // superseded by a newer track
      if (data) {
        waveformData = data;
      }
    } catch (e) {
      if (requestId !== waveformRequestId) return;
      console.error("Failed to load waveform:", e);
    } finally {
      if (requestId === waveformRequestId) {
        isLoadingWaveform = false;
        draw();
      }
    }
  }

  // Same cache-miss-triggers-full-decode cost as loadWaveform above, so it
  // gets the same request-id guard and debounce treatment.
  async function loadMoodbar(songId: number | undefined) {
    const requestId = ++moodbarRequestId;
    if (songId === undefined) {
      moodbarData = [];
      isLoadingMoodbar = false;
      draw();
      return;
    }
    isLoadingMoodbar = true;
    moodbarData = [];
    startLoadingAnimation();
    try {
      const data = await invoke<number[] | null>("get_moodbar_data", { song_id: songId, songId });
      if (requestId !== moodbarRequestId) return;
      moodbarData = data ?? [];
    } catch (e) {
      if (requestId !== moodbarRequestId) return;
      console.error("Failed to load moodbar:", e);
      moodbarData = [];
    } finally {
      if (requestId === moodbarRequestId) {
        isLoadingMoodbar = false;
        draw();
      }
    }
  }

  // Draw waveform or moodbar, depending on prefs.seekBarMode
  function draw() {
    if (!canvas || !containerEl) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const width = containerEl.clientWidth || 300;
    const height = CANVAS_BAR_HEIGHT_PX;

    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr;
      canvas.height = height * dpr;
    }

    if (ctx.save) ctx.save();
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const songLength = playerStore.currentSong?.length_nanosec || 1;
    const progressPct = playerStore.positionNanosec / songLength;

    // Dynamically read active theme colors from themeStore
    const colors = themeStore.resolvedColors;
    const accentColor = colors["color-accent"] || '#8b5cf6';
    const hoverColor = colors["color-accent-hover"] || '#a78bfa';
    const borderCol = colors["color-border"] || '#374151';

    if (prefs.seekBarMode === "moodbar") {
      drawMoodbar(ctx, width, height, progressPct, accentColor);
    } else {
      drawWaveform(ctx, width, height, progressPct, accentColor, hoverColor, borderCol);
    }
    if (ctx.restore) ctx.restore();
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
    const isPlaceholder = isLoadingWaveform || waveformData.length === 0;
    const data = isPlaceholder ? Array(150).fill(0) : waveformData;
    const numBars = data.length;
    const barGap = width < NARROW_WIDTH_BREAKPOINT_PX ? 0.5 : 1.0;
    const barWidth = Math.max(1, (width - (numBars - 1) * barGap) / numBars);

    // Premium gradients for played part
    const gradPlayed = ctx.createLinearGradient(0, height, 0, 0);
    gradPlayed.addColorStop(0, accentColor);
    gradPlayed.addColorStop(1, hoverColor);

    const textSecCol = themeStore.resolvedColors["color-text-secondary"] || '#9ca3af';

    for (let i = 0; i < numBars; i++) {
      let val: number;
      if (isPlaceholder) {
        // Animated wave pattern indicating waveform scanning/decoding in progress
        const sine = Math.sin(pulseAngle + (i / numBars) * Math.PI * 4);
        val = 0.25 + 0.2 * sine;
        ctx.fillStyle = accentColor;
        ctx.globalAlpha = 0.4 + 0.35 * Math.sin(pulseAngle + (i / numBars) * Math.PI * 3);
      } else {
        val = data[i] / 255.0;
        const barPct = i / numBars;
        if (barPct <= progressPct) {
          ctx.globalAlpha = 1.0;
          ctx.fillStyle = gradPlayed;
        } else {
          ctx.globalAlpha = 0.45;
          ctx.fillStyle = textSecCol;
        }
      }

      // Center the bars vertically
      const barHeight = Math.max(2, val * height * 0.85);
      const x = i * (barWidth + barGap);
      const y = (height - barHeight) / 2;

      if (barWidth >= 2 && ctx.roundRect) {
        ctx.beginPath();
        ctx.roundRect(x, y, barWidth, barHeight, 1);
        ctx.fill();
      } else {
        ctx.fillRect(x, y, barWidth, barHeight);
      }
    }
    ctx.globalAlpha = 1.0;
  }

  function drawMoodbar(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    progressPct: number,
    accentColor: string,
  ) {
    const totalPoints = moodbarData.length > 0 ? Math.floor(moodbarData.length / 3) : 150;

    // Downsample the raw points into broad, averaged regions instead of
    // drawing one bar per point. Individually-colored 1-2px bars read as
    // visual noise/a barcode; averaging groups of adjacent points into ~40
    // wider contiguous columns still preserves the layered low/mid/high
    // structure at a glance (verse/chorus-scale bass, vocal entrances, drum
    // transients) without the strip looking like static.
    const segmentCount = Math.min(MOODBAR_SEGMENT_COUNT, totalPoints);
    const groupSize = Math.max(1, Math.ceil(totalPoints / segmentCount));
    const segCount = Math.ceil(totalPoints / groupSize);
    const segWidth = width / segCount;

    // Still decoding/generating (or nothing loaded yet for this track): show
    // the same pulsing scan animation as drawWaveform's placeholder, instead
    // of silently rendering nothing (all-zero bands) while data is in flight.
    const isPlaceholder = isLoadingMoodbar || moodbarData.length === 0;

    for (let s = 0; s < segCount; s++) {
      const x = s * segWidth;
      const w = segWidth + 0.5;

      if (isPlaceholder) {
        const sine = Math.sin(pulseAngle + (s / segCount) * Math.PI * 4);
        const barH = Math.max(2, (0.25 + 0.2 * sine) * height * 0.85);
        ctx.globalAlpha = 0.4 + 0.35 * Math.sin(pulseAngle + (s / segCount) * Math.PI * 3);
        ctx.fillStyle = accentColor;
        ctx.fillRect(x, (height - barH) / 2, w, barH);
        continue;
      }

      const start = s * groupSize;
      const end = Math.min(start + groupSize, totalPoints);

      let low = 0;
      let mid = 0;
      let high = 0;
      if (moodbarData.length > 0) {
        let n = 0;
        for (let i = start; i < end; i++) {
          low += moodbarData[i * 3];
          mid += moodbarData[i * 3 + 1];
          high += moodbarData[i * 3 + 2];
          n++;
        }
        low /= n;
        mid /= n;
        high /= n;
      }

      const segPct = s / segCount;
      const played = segPct <= progressPct;
      // Unplayed columns are dimmed as a whole (not blended toward a
      // per-theme gray — these are fixed structural colors, not mood-derived
      // ones) so the progress line remains the only cue that changes hue.
      const alpha = played ? 1.0 : 0.4;
      const center = height / 2;

      // Blue low-band layer: the outer envelope, centered and symmetric
      // top/bottom like a standard audio waveform — this is the layer
      // expected to be present almost continuously (bassline), so it reads
      // as the strip's base shape.
      const lowH = Math.max(2, (low / 255) * height * 0.9);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = LOW_BAND_COLOR;
      ctx.fillRect(x, center - lowH / 2, w, lowH);

      // Amber mid-band layer: overlaid on top of the blue envelope, same
      // center line, narrower. Appearing/disappearing here is what makes
      // vocal entrances and instrumental (vocal-free) sections legible,
      // independent of the bassline underneath.
      const midH = (mid / 255) * height * 0.75;
      if (midH > 1.5) {
        ctx.globalAlpha = alpha * 0.85;
        ctx.fillStyle = MID_BAND_COLOR;
        ctx.fillRect(x, center - midH / 2, w, midH);
      }

      // White high-band layer: thin spike markers at the top and bottom
      // tips of the treble envelope, not a filled bar — this is what reads
      // as a sharp transient (hi-hat/cymbal hit) rather than competing with
      // the amber layer's area underneath it.
      const highH = (high / 255) * height;
      if (highH > 1.5) {
        ctx.globalAlpha = alpha * 0.95;
        ctx.fillStyle = HIGH_BAND_COLOR;
        ctx.fillRect(x, center - highH / 2 - 1, w, 2);
        ctx.fillRect(x, center + highH / 2 - 1, w, 2);
      }

      if (played) {
        ctx.globalAlpha = 1.0;
        ctx.fillStyle = accentColor;
        ctx.fillRect(x, 0, w, 1.5);
      }
    }
    ctx.globalAlpha = 1.0;
  }

  // React to changes in currentSong (or a mode toggle) using Svelte 5
  // $effect. Debounced: the cleanup callback cancels the pending timer
  // whenever songId/mode changes again before it fires, so a burst of rapid
  // skips only ever loads data for whichever track the user actually
  let loadedSongId: number | undefined = undefined;
  let loadedMode: string | undefined = undefined;

  // Fetch waveform or moodbar when song or mode actually changes.
  $effect(() => {
    const songId = playerStore.currentSong?.id;
    const mode = prefs.seekBarMode;

    if (songId !== loadedSongId || mode !== loadedMode) {
      loadedSongId = songId;
      loadedMode = mode;
      if (mode === "moodbar") {
        moodbarData = [];
        loadMoodbar(songId);
      } else {
        waveformData = [];
        loadWaveform(songId);
      }
    }
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
  bind:this={containerEl}
  onmousedown={handleMouseDown}
  class="relative flex-1 h-7 overflow-hidden cursor-pointer flex items-center group select-none"
  title={prefs.seekBarMode === 'moodbar'
    ? i18n.t('playerBar.moodbarLegend', {}, 'Frequency bands — blue = low (bass), amber = mid (vocals/snares/leads), white = high (hi-hats/cymbals/transients); taller bands carry more energy')
    : undefined}
>
  <canvas bind:this={canvas} class="block w-full h-7 opacity-100"></canvas>
</div>
