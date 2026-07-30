<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import { AlertTriangle, CheckCircle2, Info, X } from "lucide-svelte";
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
      class="pointer-events-auto flex items-center gap-2.5 px-4 py-2.5 rounded-xl border shadow-2xl text-sm font-medium max-w-md {toast.variant === 'error'
        ? 'bg-red-500/10 border-red-500/30 text-red-400'
        : 'bg-brand-sidebar border-brand-border text-brand-text-primary'}"
    >
      {#if toast.variant === "error"}
        <AlertTriangle class="w-4 h-4 shrink-0" />
      {:else if toast.variant === "success"}
        <span class="relative inline-flex w-4 h-4 shrink-0 items-center justify-center">
          <span class="absolute inset-0 rounded-full anim-glow-ring"></span>
          <CheckCircle2 class="w-4 h-4 text-brand-accent-text anim-check-pop" />
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
