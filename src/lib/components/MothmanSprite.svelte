<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  // Sprite sheet is 16 frames of 472x452 (each pose isolated to its own
  // silhouette, then centered on a shared canvas so poses of different
  // sizes don't shift position frame-to-frame), laid out horizontally:
  //   0-2   walk_a/b/c   standing/stepping poses (walk cycle + idle bob)
  //   3-7   flap_a..e    wing-flap flutter sequence
  //   8-10  peek_a..c    turns around to look behind, then back
  //   11    blink        eyes-closing transition into vibe/rest
  //   12    vibe         headphones on, eyes closed
  //   13    rest         curled up, sleeping
  //   14    leap         side-profile wings-raised hop, facing right
  //   15    recover      landing pose after the leap
  //   16    flap_c_flip  flap_c (5) mirrored to face left
  //   17    leap_flip    leap (14) mirrored to face left
  // Displayed well below native resolution, so image-rendering: pixelated
  // keeps the pixel-art edges crisp on the downscale.
  const FRAME_COUNT = 18;
  const FRAME_ASPECT = 472 / 452;
  const DISPLAY_H = 72;
  const DISPLAY_W = Math.round(DISPLAY_H * FRAME_ASPECT);

  const WALK_FRAMES = [0, 1, 2];
  const ACTIONS: number[][] = [
    [3, 4, 5, 6, 7, 4], // flap: flutter through the wing-spread sequence
    [8, 9, 10, 9], // peek: turn around to look behind, then back
    [11, 12, 12, 11], // vibe: close eyes, headphones on, then open
    [11, 13, 13, 11], // rest: close eyes, curl up to sleep, then open
    [14, 15], // leap: hop sideways, land
  ];

  // Longer walks use a faster "run" cycle (frame 5 into the leap pose) —
  // these frames are pre-flipped per direction rather than mirrored with
  // scaleX, so the run doesn't need the whole-sprite flip transform.
  const RUN_DISTANCE_PX = 140;
  const RUN_FRAMES_RIGHT = [5, 14];
  const RUN_FRAMES_LEFT = [16, 17];

  const WALK_SPEED_PX_S = 26;
  const RUN_SPEED_PX_S = 68;
  const WALK_FRAME_MS = 220;
  const RUN_FRAME_MS = 150;
  const IDLE_FRAME_MS = 750;
  const ACTION_FRAME_MS = 450;
  const PAUSE_MIN_MS = 4000;
  const PAUSE_MAX_MS = 11000;
  const ACTION_CHANCE = 0.6;
  const SIT_FRAME = 13; // "rest" pose, held while playback isn't active

  let { isPlaying = true }: { isPlaying?: boolean } = $props();

  let containerWidth = $state(0);
  let x = $state(0);
  let frame = $state(0);
  let facingLeft = $state(false);
  let isRunning = $state(false);

  let rafId: number | undefined;
  let frameTimer: ReturnType<typeof setInterval> | undefined;
  let stateTimer: ReturnType<typeof setTimeout> | undefined;

  function maxX() {
    return Math.max(0, containerWidth - DISPLAY_W);
  }

  function walkTo(targetX: number) {
    const startX = x;
    const distance = Math.abs(targetX - startX);
    if (distance < 1) {
      pause();
      return;
    }
    facingLeft = targetX < startX;
    isRunning = distance > RUN_DISTANCE_PX;
    const speed = isRunning ? RUN_SPEED_PX_S : WALK_SPEED_PX_S;
    const frameMs = isRunning ? RUN_FRAME_MS : WALK_FRAME_MS;
    const cycleFrames = isRunning ? (facingLeft ? RUN_FRAMES_LEFT : RUN_FRAMES_RIGHT) : WALK_FRAMES;
    const duration = (distance / speed) * 1000;
    const startTime = performance.now();

    let stepIdx = 0;
    clearInterval(frameTimer);
    frameTimer = setInterval(() => {
      stepIdx = (stepIdx + 1) % cycleFrames.length;
      frame = cycleFrames[stepIdx];
    }, frameMs);

    function tick(now: number) {
      const t = Math.min(1, (now - startTime) / duration);
      x = startX + (targetX - startX) * t;
      if (t < 1) {
        rafId = requestAnimationFrame(tick);
      } else {
        clearInterval(frameTimer);
        isRunning = false;
        pause();
      }
    }
    rafId = requestAnimationFrame(tick);
  }

  function playAction() {
    const sequence = ACTIONS[Math.floor(Math.random() * ACTIONS.length)];
    let i = 0;
    frame = sequence[0];
    clearInterval(frameTimer);
    frameTimer = setInterval(() => {
      i += 1;
      if (i < sequence.length) {
        frame = sequence[i];
      } else {
        clearInterval(frameTimer);
        walkTo(Math.random() * maxX());
      }
    }, ACTION_FRAME_MS);
  }

  function pause() {
    isRunning = false;
    let idleIdx = 0;
    frame = WALK_FRAMES[0];
    clearInterval(frameTimer);
    frameTimer = setInterval(() => {
      idleIdx = (idleIdx + 1) % WALK_FRAMES.length;
      frame = WALK_FRAMES[idleIdx];
    }, IDLE_FRAME_MS);

    const dwell = PAUSE_MIN_MS + Math.random() * (PAUSE_MAX_MS - PAUSE_MIN_MS);
    stateTimer = setTimeout(() => {
      clearInterval(frameTimer);
      if (Math.random() < ACTION_CHANCE) {
        playAction();
      } else {
        walkTo(Math.random() * maxX());
      }
    }, dwell);
  }

  function freeze() {
    if (rafId !== undefined) cancelAnimationFrame(rafId);
    clearInterval(frameTimer);
    clearTimeout(stateTimer);
    isRunning = false;
    frame = SIT_FRAME;
  }

  onMount(() => {
    x = Math.random() * maxX();
  });

  // Stop wandering and sit still whenever playback isn't active; resumes
  // the wander/action loop from wherever it's standing when it picks back up.
  $effect(() => {
    if (isPlaying) {
      pause();
    } else {
      freeze();
    }
  });

  onDestroy(() => {
    if (rafId !== undefined) cancelAnimationFrame(rafId);
    clearInterval(frameTimer);
    clearTimeout(stateTimer);
  });
</script>

<div bind:clientWidth={containerWidth} class="mothman-wander" aria-hidden="true">
  <div
    class="mothman-sprite"
    style="width:{DISPLAY_W}px; height:{DISPLAY_H}px; background-size:{DISPLAY_W * FRAME_COUNT}px {DISPLAY_H}px; background-position:{-frame * DISPLAY_W}px 0; transform: translateX({x}px) scaleX({!isRunning && facingLeft ? -1 : 1});"
  ></div>
</div>

<style>
  .mothman-wander {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 100%;
    height: 0;
    pointer-events: none;
  }

  .mothman-sprite {
    position: absolute;
    bottom: -6px;
    left: 0;
    background-image: url('/mothman-sprite.png');
    background-repeat: no-repeat;
    image-rendering: pixelated;
    pointer-events: none;
    user-select: none;
    transform-origin: bottom center;
    /* Same red accent glow as the playbar's own --glass-glow (theme.svelte.ts),
       reimplemented as drop-shadow (not box-shadow) so it follows the sprite's
       silhouette instead of its rectangular bounding box. */
    filter: drop-shadow(0 0 6px rgba(198, 19, 61, 0.55)) drop-shadow(0 0 16px rgba(198, 19, 61, 0.35));
  }
</style>
