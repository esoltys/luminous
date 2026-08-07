<script lang="ts">
  // Dev-only gallery for previewing every micro-animation from
  // docs/luminous-micro-animations.dc.html (see issue #182) on demand,
  // including moments that are rare or slow to reach in a real library
  // (first launch, milestones, an actual import finishing).
  //
  // "Favouriting a song" and "Import finished" render the real
  // HeartToggle/Toast components so this stays honest with production
  // behaviour; the rest are demo markup for moments not wired into the app
  // yet (see the checklist in issue #182).
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import HeartToggle from "../../../lib/components/HeartToggle.svelte";
  import { toastStore } from "../../../lib/stores/toast.svelte";
  import { Folder, ListMusic, ArrowUp, AlertTriangle } from "lucide-svelte";

  // Most dev machines with Windows "Animation effects" turned off (or any
  // prefers-reduced-motion setting) will otherwise only ever see the
  // reduced-motion fallback here — that's animations.css doing its job, but
  // it defeats the point of a preview gallery. `force-full-motion` on <html>
  // is the escape hatch defined alongside the media query in animations.css.
  let prefersReducedMotion = $state(false);
  let forceFullMotion = $state(false);

  onMount(() => {
    if (!import.meta.env.DEV) {
      goto("/");
      return;
    }
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    prefersReducedMotion = query.matches;
    forceFullMotion = query.matches;
    const onChange = (e: MediaQueryListEvent) => { prefersReducedMotion = e.matches; };
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  });

  $effect(() => {
    document.documentElement.classList.toggle("force-full-motion", forceFullMotion);
  });

  let favoriteDemo = $state(false);
  function replayFavorite() {
    favoriteDemo = false;
    requestAnimationFrame(() => { favoriteDemo = true; });
  }

  function replayToast() {
    toastStore.show("128 tracks added", "success");
  }

  function replayMilestoneToast() {
    toastStore.show("1,000 songs in your library!", "milestone");
  }

  function replayWarningToast() {
    toastStore.show("3 files couldn\u2019t be read", "warning");
  }

  function replayQueueToast() {
    toastStore.show("Golden Hour complete", "milestone");
  }

  function replayWelcomeToast() {
    toastStore.show("Welcome to Luminous v1.0.0!", "milestone");
  }

  let replayCounters = $state<Record<string, number>>({
    organizing: 0,
    folder: 0,
    playlist: 0,
    autoPlaylist: 0,
    release: 0,
    milestone: 0,
    queue: 0,
    firstLaunch: 0,
    warning: 0,
  });
  function replay(name: string) {
    replayCounters[name]++;
  }

  const tierColor: Record<string, string> = {
    Milestone: "text-brand-gold bg-[color:rgb(255_182_72_/_0.14)]",
    New: "text-brand-accent-text bg-brand-accent/15",
    Success: "text-[#8fbf9a] bg-[#8fbf9a]/15",
    Warning: "text-[#e0a0a0] bg-[#e0a0a0]/15",
  };
</script>

<div class="h-full overflow-y-auto p-8 bg-brand-main text-brand-text-primary">
  <div class="max-w-5xl mx-auto space-y-2 mb-8">
    <h1 class="text-2xl font-bold">Micro-animation gallery</h1>
    <p class="text-sm text-brand-text-secondary max-w-2xl">
      Dev-only preview for the moments in
      <a href="https://github.com/esoltys/luminous/issues/182" class="text-brand-accent-text hover:underline" target="_blank" rel="noreferrer">issue #182</a>.
      Click Replay on any card to trigger it on demand — useful for milestones and other moments that are rare or slow to reach for real.
    </p>
    {#if prefersReducedMotion}
      <div class="flex items-center justify-between gap-4 bg-brand-accent/10 border border-brand-accent/30 rounded-xl px-4 py-3 max-w-2xl">
        <p class="text-xs text-brand-text-secondary">
          Your OS has "reduce motion" on, so every card below is correctly collapsing to the spec's reduced-motion fallback (plain fade, or nothing for rings/glows) — that's intentional, not broken. Toggle below to preview full motion anyway.
        </p>
        <label class="flex items-center gap-2 text-xs font-semibold shrink-0 select-none">
          <input type="checkbox" bind:checked={forceFullMotion} class="accent-[var(--color-accent)]" />
          Preview full motion
        </label>
      </div>
    {/if}
  </div>

  <div class="max-w-5xl mx-auto grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">

    <!-- 1. Favouriting a song (real component) -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">Favouriting a song</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.Success}">Success</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg">
        <HeartToggle favorite={favoriteDemo} onToggle={() => (favoriteDemo = !favoriteDemo)} sizeClass="w-6 h-6" />
      </div>
      <button onclick={replayFavorite} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 2. Import finished (real Toast component, mounted in +layout.svelte) -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">Import finished</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.Success}">Success</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg text-xs text-brand-text-secondary text-center px-3">
        Watch the bottom of the screen
      </div>
      <button onclick={replayToast} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 3. Organizing complete -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">Organizing complete</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.Success}">Success</span>
      </div>
      <div class="h-16 flex flex-col gap-1.5 justify-center bg-brand-main rounded-lg px-4">
        {#key replayCounters.organizing}
          {#each [0, 50, 100] as delay}
            <div
              class="h-2 rounded anim-row-sweep"
              style="background: linear-gradient(90deg, #1c1f29 25%, #2f3a4d 50%, #1c1f29 75%); background-size: 250% 100%; animation-delay: {delay}ms;"
            ></div>
          {/each}
        {/key}
      </div>
      <button onclick={() => replay('organizing')} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 4. First watched folder -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">First watched folder</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.New}">New</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg">
        {#key replayCounters.folder}
          <div class="relative w-8 h-8 flex items-center justify-center">
            <span class="absolute inset-0 rounded-lg anim-ring-expand"></span>
            <Folder class="w-6 h-6 text-brand-accent-text anim-folder-pop" />
          </div>
        {/key}
      </div>
      <button onclick={() => replay('folder')} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 5. Creating a new playlist -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">Creating a new playlist</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.New}">New</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg">
        {#key replayCounters.playlist}
          <div class="w-24 h-11 bg-brand-sidebar border border-brand-border rounded-lg p-2 anim-card-materialize anim-border-glow-pulse flex flex-col gap-1.5 justify-center">
            <div class="w-3/5 h-1.5 rounded bg-brand-accent"></div>
            <div class="w-4/5 h-1.5 rounded bg-brand-border"></div>
          </div>
        {/key}
      </div>
      <button onclick={() => replay('playlist')} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 6. New auto-playlist appears -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">New auto-playlist appears</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.New}">New</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg">
        {#key replayCounters.autoPlaylist}
          <div class="flex items-center gap-2 anim-slide-in-fade">
            <ListMusic class="w-3.5 h-3.5 text-brand-accent-text" />
            <span class="text-xs font-semibold">Deep Focus</span>
            <span class="text-[9px] font-bold uppercase tracking-wide rounded px-1.5 py-0.5 text-brand-gold bg-[color:rgb(255_182_72_/_0.14)] anim-badge-glow">Auto</span>
          </div>
        {/key}
      </div>
      <button onclick={() => replay('autoPlaylist')} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 7. New release available — a new Luminous version, not a new song/album; shown in Settings → About -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">New release available <span class="text-brand-text-secondary font-normal">(Settings → About)</span></span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.New}">New</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg">
        {#key replayCounters.release}
          <div class="bg-brand-accent/10 border border-brand-accent/30 rounded-lg px-3 py-2 flex items-center gap-2.5 anim-card-materialize">
            <div class="relative w-7 h-7 rounded-lg bg-brand-accent/20 text-brand-accent-text border border-brand-accent/30 flex items-center justify-center shrink-0">
              <span class="absolute -top-1 -right-1 w-2 h-2 rounded-full bg-brand-accent anim-badge-glow"></span>
              <ArrowUp class="w-3.5 h-3.5 stroke-[2.5]" />
            </div>
            <span class="text-xs font-bold">Luminous 1.2.0 available</span>
          </div>
        {/key}
      </div>
      <button onclick={() => replay('release')} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 8. Reaching a milestone -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">Reaching a milestone</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.Milestone}">Milestone</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg relative">
        {#key replayCounters.milestone}
          <span class="absolute w-16 h-9 rounded-full anim-gold-ring"></span>
          <span class="text-lg font-black text-brand-gold anim-milestone-bounce">1,000 songs</span>
        {/key}
      </div>
      <button onclick={() => { replay('milestone'); replayMilestoneToast(); }} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 9. Completing a queue -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">Completing a queue</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.Milestone}">Milestone</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg">
        {#key replayCounters.queue}
          <span
            class="text-sm font-semibold bg-clip-text text-transparent anim-shimmer-sweep"
            style="background-image: linear-gradient(90deg, #f1f3f8 40%, #FFB648 50%, #f1f3f8 60%); background-size: 250% 100%;"
          >Queue complete — nice listening</span>
        {/key}
      </div>
      <button onclick={() => { replay('queue'); replayQueueToast(); }} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 10. First launch -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">First launch</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.Milestone}">Milestone</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg relative">
        {#key replayCounters.firstLaunch}
          <span class="absolute w-24 h-11 rounded-full anim-gold-ring"></span>
          <div class="flex items-center gap-2 anim-milestone-bounce">
            <img src="/luminous-mark.svg" alt="" class="w-8 h-8" />
            <span class="text-lg font-black tracking-tight text-brand-gold">Luminous</span>
          </div>
        {/key}
      </div>
      <button onclick={() => { replay('firstLaunch'); replayWelcomeToast(); }} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

    <!-- 11. Import needs attention -->
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <span class="font-semibold text-sm">Import needs attention</span>
        <span class="text-[10px] font-bold uppercase tracking-wide rounded-full px-2 py-0.5 {tierColor.Warning}">Warning</span>
      </div>
      <div class="h-16 flex items-center justify-center bg-brand-main rounded-lg">
        {#key replayCounters.warning}
          <div class="flex items-center gap-2 rounded-lg px-3 py-2 anim-warn-shake anim-warn-flash">
            <AlertTriangle class="w-3.5 h-3.5 text-[#e0a0a0]" />
            <span class="text-xs font-semibold">3 files couldn't be read</span>
          </div>
        {/key}
      </div>
      <button onclick={() => { replay('warning'); replayWarningToast(); }} class="text-xs font-semibold text-brand-accent-text border border-brand-accent/40 rounded-full px-3 py-1.5 self-start hover:bg-brand-accent/10">▸ Replay</button>
    </div>

  </div>
</div>
