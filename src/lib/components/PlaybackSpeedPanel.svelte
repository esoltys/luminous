<script lang="ts">
  import { onDestroy } from "svelte";
  import { MinusIcon as Minus, PlusIcon as Plus } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import {
    PLAYBACK_SPEED_MAX,
    PLAYBACK_SPEED_MIN,
    PLAYBACK_SPEED_NORMAL,
    PLAYBACK_SPEED_PRESETS,
    PLAYBACK_SPEED_STEP,
    formatPlaybackSpeed,
    formatPlaybackSpeedShort,
    snapPlaybackSpeed,
  } from "../playbackSpeed";

  // Each change restarts the audio at the current position, so a drag is applied once the
  // thumb has rested for DRAG_APPLY_MS, and again on release; the readout follows the thumb
  // meanwhile.
  const DRAG_APPLY_MS = 120;

  let dragSpeed = $state<number | null>(null);
  let applyTimer: ReturnType<typeof setTimeout> | null = null;

  const shown = $derived(dragSpeed ?? playerStore.playbackSpeed);
  const fillPercent = $derived(
    ((shown - PLAYBACK_SPEED_MIN) / (PLAYBACK_SPEED_MAX - PLAYBACK_SPEED_MIN)) * 100,
  );

  function cancelTimer() {
    if (applyTimer) clearTimeout(applyTimer);
    applyTimer = null;
  }

  function applyNow(speed: number) {
    cancelTimer();
    void playerStore.setPlaybackSpeed(speed);
  }

  function handleInput(e: Event) {
    const speed = snapPlaybackSpeed(parseFloat((e.target as HTMLInputElement).value));
    dragSpeed = speed;
    cancelTimer();
    applyTimer = setTimeout(() => applyNow(speed), DRAG_APPLY_MS);
  }

  function handleChange(e: Event) {
    applyNow(snapPlaybackSpeed(parseFloat((e.target as HTMLInputElement).value)));
    dragSpeed = null;
  }

  function step(direction: 1 | -1) {
    applyNow(snapPlaybackSpeed(shown + direction * PLAYBACK_SPEED_STEP));
  }

  onDestroy(cancelTimer);
</script>

<div class="px-3 py-2 flex flex-col gap-3" data-testid="playback-speed-panel">
  <div class="text-center text-lg font-semibold tabular-nums" aria-live="polite">
    {formatPlaybackSpeed(shown)}
  </div>

  <div class="flex items-center gap-2">
    <button
      type="button"
      class="speed-step bg-brand-main text-brand-text-primary hover:enabled:bg-brand-border"
      aria-label={i18n.t("playerBar.playbackSpeedSlower")}
      title={i18n.t("playerBar.playbackSpeedSlower")}
      disabled={shown <= PLAYBACK_SPEED_MIN}
      onclick={() => step(-1)}
    >
      <Minus class="w-3.5 h-3.5" weight="regular" />
    </button>
    <input
      type="range"
      class="speed-slider flex-1 h-1 rounded-lg outline-none"
      min={PLAYBACK_SPEED_MIN}
      max={PLAYBACK_SPEED_MAX}
      step={PLAYBACK_SPEED_STEP}
      value={shown}
      oninput={handleInput}
      onchange={handleChange}
      style="background: linear-gradient(to right, var(--color-accent) {fillPercent}%, var(--color-border) {fillPercent}%)"
      aria-label={i18n.t("playerBar.playbackSpeedSlider")}
    />
    <button
      type="button"
      class="speed-step bg-brand-main text-brand-text-primary hover:enabled:bg-brand-border"
      aria-label={i18n.t("playerBar.playbackSpeedFaster")}
      title={i18n.t("playerBar.playbackSpeedFaster")}
      disabled={shown >= PLAYBACK_SPEED_MAX}
      onclick={() => step(1)}
    >
      <Plus class="w-3.5 h-3.5" weight="regular" />
    </button>
  </div>

  <div class="flex flex-nowrap justify-center gap-1.5">
    {#each PLAYBACK_SPEED_PRESETS as preset (preset)}
      <button
        type="button"
        class="speed-preset label-small whitespace-nowrap {shown === preset
          ? 'bg-brand-accent text-brand-accent-contrast'
          : 'bg-brand-main text-brand-text-primary hover:bg-brand-border'}"
        aria-pressed={shown === preset}
        onclick={() => applyNow(preset)}
      >
        {formatPlaybackSpeedShort(preset)}
        {#if preset === PLAYBACK_SPEED_NORMAL}
          <span class="opacity-70">{i18n.t("playerBar.playbackSpeedNormal")}</span>
        {/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .speed-step {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    flex-shrink: 0;
    border-radius: 9999px;
    transition: background-color 0.15s ease;
  }
  .speed-step:disabled {
    opacity: 0.4;
  }
  .speed-preset {
    padding: 0.25rem 0.625rem;
    border-radius: 9999px;
    transition: background-color 0.15s ease;
  }

  .speed-slider {
    -webkit-appearance: none;
    appearance: none;
  }
  .speed-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ffffff;
    border: 2px solid var(--color-accent);
  }
  .speed-slider::-moz-range-thumb {
    width: 12px;
    height: 12px;
    border: 2px solid var(--color-accent);
    border-radius: 50%;
    background: #ffffff;
  }
</style>
