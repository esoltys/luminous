<script lang="ts">
  import { playerStore, type CompletedSession } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { shouldSkipGlobalShortcut } from "../utils/globalShortcuts";
  import {
    CheckCircleIcon as CheckCircle,
    ShuffleIcon as Shuffle,
    ArrowCounterClockwiseIcon as RotateCcw,
    ArrowsInIcon as Minimize2,
    CloudArrowUpIcon as UploadCloud
  } from "phosphor-svelte";

  let { session }: { session: CompletedSession } = $props();

  let title = $derived(
    session.contextName && session.contextName.toLowerCase() !== "queue"
      ? i18n.t("immersive.contextComplete", { context: session.contextName }, `${session.contextName} Complete`)
      : i18n.t("immersive.queueComplete", {}, "Queue Complete")
  );

  let subtitle = $derived(
    session.trackCount === 1
      ? i18n.t("immersive.trackPlayed", {}, "1 track played")
      : i18n.t("immersive.tracksPlayed", { count: session.trackCount }, `${session.trackCount} tracks played`)
  );

  async function handleShuffleLibrary() {
    let songs = collectionStore.songs;
    if (songs.length === 0) {
      await collectionStore.refreshLibrary();
      songs = collectionStore.songs;
    }
    if (songs.length > 0) {
      await playerStore.shuffleLibrary(songs);
    }
  }

  async function handleReplay() {
    await playerStore.replayCompletedSession();
  }

  function handleExit() {
    playerStore.completedSession = null;
    windowLayoutStore.exitImmersiveMode();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!windowLayoutStore.effectiveImmersiveMode) return;
    if (shouldSkipGlobalShortcut(e)) return;

    if (e.key === "Escape") {
      e.preventDefault();
      handleExit();
    } else if (e.code === "Space" || e.key === " ") {
      e.preventDefault();
      handleReplay();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="flex flex-col items-center justify-center text-center max-w-2xl mx-auto px-4 py-8 select-none z-10 w-full" data-testid="immersive-session-wrap">
  <!-- Celebration Milestone Badge -->
  <div class="w-20 h-20 md:w-24 md:h-24 rounded-full bg-brand-gold/15 border border-brand-gold/40 flex items-center justify-center text-brand-gold shadow-[0_0_35px_rgba(245,158,11,0.25)] anim-gold-ring mb-6">
    <CheckCircle class="w-10 h-10 md:w-12 md:h-12 anim-check-pop" weight="fill" />
  </div>

  <!-- Contextual Completion Title -->
  <h1 class="text-3xl sm:text-4xl md:text-5xl lg:text-6xl font-heading font-bold text-brand-text-primary tracking-tight text-balance leading-tight">
    {title}
  </h1>

  <!-- Completed Tracks Subtitle -->
  <p class="text-base sm:text-lg md:text-xl text-brand-text-secondary font-medium mt-3 mb-8">
    {subtitle}
  </p>

  <!-- Continuation Action Buttons -->
  <div class="flex flex-wrap items-center justify-center gap-3 sm:gap-4 mb-6">
    <!-- Shuffle Library (Primary Action) -->
    <button
      type="button"
      onclick={handleShuffleLibrary}
      class="px-5 py-2.5 sm:px-6 sm:py-3 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-text font-semibold flex items-center gap-2.5 shadow-lg shadow-brand-accent/25 hover:scale-[1.03] active:scale-[0.97] transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-brand-accent focus:outline-none"
    >
      <Shuffle class="w-4 h-4 sm:w-5 sm:h-5" weight="bold" />
      <span>{i18n.t('immersive.shuffleLibrary', {}, 'Shuffle Library')}</span>
    </button>

    <!-- Replay Session (Secondary Action) -->
    <button
      type="button"
      onclick={handleReplay}
      class="px-5 py-2.5 sm:px-6 sm:py-3 rounded-full bg-brand-surface/90 hover:bg-brand-surface border border-brand-border/80 hover:border-brand-text-secondary/40 text-brand-text-primary font-semibold flex items-center gap-2.5 shadow-sm hover:scale-[1.03] active:scale-[0.97] transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-brand-accent focus:outline-none"
    >
      <RotateCcw class="w-4 h-4 sm:w-5 sm:h-5" weight="bold" />
      <span>{i18n.t('immersive.replay', {}, 'Replay')}</span>
    </button>

    <!-- Exit Immersive (Tertiary Action) -->
    <button
      type="button"
      onclick={handleExit}
      class="px-4 py-2.5 sm:px-5 sm:py-3 rounded-full bg-brand-surface/40 hover:bg-brand-surface/80 border border-brand-border/60 text-brand-text-secondary hover:text-brand-text-primary font-medium flex items-center gap-2 hover:scale-[1.03] active:scale-[0.97] transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-brand-accent focus:outline-none"
    >
      <Minimize2 class="w-4 h-4" weight="bold" />
      <span>{i18n.t('immersive.exitImmersive', {}, 'Exit Immersive')}</span>
    </button>
  </div>

  <!-- Drag-and-drop file invitation cue -->
  <div class="flex items-center gap-2 text-xs sm:text-sm text-brand-text-secondary/60 select-none">
    <UploadCloud class="w-4 h-4" />
    <span>{i18n.t('immersive.dropToPlay', {}, 'Drop tracks anywhere to play')}</span>
  </div>
</div>
