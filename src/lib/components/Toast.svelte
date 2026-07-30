<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import { AlertTriangle, CheckCircle2, Info, Star, Sparkles, X } from "lucide-svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { portal } from "../utils/portal";
</script>

<div
  use:portal
  class="fixed top-24 right-4 z-[100] flex flex-col items-end gap-2 pointer-events-none px-4"
>
  {#each toastStore.messages as toast (toast.id)}
    <div
      in:fly={{ x: 24, duration: 200 }}
      out:fade={{ duration: 150 }}
      class="pointer-events-auto flex items-center gap-2.5 px-4 py-2.5 rounded-xl border shadow-2xl backdrop-blur-md text-sm font-semibold max-w-md
        {toast.variant === 'error'
          ? 'bg-[#1f1013] border-red-500/50 text-red-400 anim-warn-shake'
          : toast.variant === 'warning'
            ? 'bg-[#1f1a10] border-amber-500/50 text-amber-400 anim-warn-shake'
            : toast.variant === 'milestone'
              ? 'bg-[#1f1a12] border-brand-gold/50 text-brand-gold'
              : 'bg-brand-sidebar border-brand-border text-brand-text-primary'}"
    >
      {#if toast.variant === "error" || toast.variant === "warning"}
        <AlertTriangle class="w-4 h-4 shrink-0" />
      {:else if toast.variant === "success"}
        <span class="relative inline-flex w-4 h-4 shrink-0 items-center justify-center">
          <span class="absolute inset-0 rounded-full anim-glow-ring"></span>
          <CheckCircle2 class="w-4 h-4 text-brand-accent-text anim-check-pop" />
        </span>
      {:else if toast.variant === "milestone"}
        <span class="relative inline-flex w-5 h-5 shrink-0 items-center justify-center">
          <span class="absolute inset-0 rounded-full anim-gold-ring"></span>
          <Star class="w-5 h-5 text-brand-gold fill-brand-gold/30 anim-milestone-bounce" />
        </span>
      {:else}
        <Info class="w-4 h-4 shrink-0 text-brand-accent-text" />
      {/if}
      <span class="flex-1">{toast.text}</span>
      <button
        onclick={() => toastStore.dismiss(toast.id)}
        class="shrink-0 opacity-60 hover:opacity-100 transition-opacity cursor-pointer"
      >
        <X class="w-3.5 h-3.5" />
      </button>
    </div>
  {/each}
</div>
