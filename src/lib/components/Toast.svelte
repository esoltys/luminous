<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import {
    WarningIcon as AlertTriangle,
    CheckCircleIcon as CheckCircle2,
    InfoIcon as Info,
    ChecksIcon as CheckCheck,
    SparkleIcon as Sparkles,
    XIcon as X,
    ClipboardIcon as Clipboard,
    CheckIcon as Check,
    ArrowsClockwiseIcon as RefreshCw
  } from "phosphor-svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";
  import Button from "./Button.svelte";

  const COPY_FEEDBACK_DURATION_MS = 1500;
  let copiedToastId = $state<number | null>(null);
  let copyTimeout: ReturnType<typeof setTimeout> | null = null;

  async function openExternalUrl(url: string) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank");
    }
  }

  async function copyToClipboard(id: number, text: string) {
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        const textArea = document.createElement("textarea");
        textArea.value = text;
        textArea.style.position = "fixed";
        textArea.style.left = "-999999px";
        textArea.style.top = "-999999px";
        document.body.appendChild(textArea);
        textArea.focus();
        textArea.select();
        document.execCommand("copy");
        textArea.remove();
      }
      if (copyTimeout) clearTimeout(copyTimeout);
      copiedToastId = id;
      copyTimeout = setTimeout(() => {
        if (copiedToastId === id) copiedToastId = null;
      }, COPY_FEEDBACK_DURATION_MS);
    } catch (err) {
      console.error("Failed to copy error to clipboard:", err);
    }
  }

  $effect(() => {
    return () => {
      if (copyTimeout) clearTimeout(copyTimeout);
    };
  });
</script>

<!-- From 2xl up, anchored inside the 80px (h-20) TopNavigation header just left
     of the w-16 logo, so a single toast sits in the header's empty space
     instead of covering page content. Narrower than that, the flex-1
     max-w-2xl search box leaves too little room for a max-w-md toast, so drop
     back to just under the header. -->
<div
  use:portal
  class="fixed top-24 right-4 2xl:top-3 2xl:right-24 z-[100] flex flex-col items-end gap-2 pointer-events-none px-4"
>
  {#each toastStore.messages as toast (toast.id)}
    <div
      in:fly={{ x: 24, duration: 200 }}
      out:fade={{ duration: 150 }}
      class="pointer-events-auto flex items-start gap-2.5 px-4 py-2.5 rounded-xl border shadow-2xl backdrop-blur-md text-sm font-semibold max-w-md
        {toast.variant === 'error'
          ? 'bg-[#1f1013] border-red-500/50 text-red-400 anim-warn-shake'
          : toast.variant === 'warning'
            ? 'bg-[#1f1a10] border-amber-500/50 text-amber-400 anim-warn-shake'
            : toast.variant === 'milestone'
              ? 'bg-[#1f1a12] border-brand-gold/50 text-brand-gold'
              : 'bg-brand-sidebar border-brand-border text-brand-text-primary'}"
    >
      {#if toast.task}
        <div class="relative w-4 h-4 shrink-0 flex items-center justify-center translate-y-[calc((1lh-1rem)/2)]">
          {#if toast.task.status === "running"}
            {#if typeof toast.task.progress === "number"}
              <svg class="w-4 h-4 -rotate-90 origin-center shrink-0" viewBox="0 0 20 20">
                <circle
                  cx="10"
                  cy="10"
                  r="7.5"
                  class="stroke-brand-border/40 fill-none"
                  stroke-width="2.5"
                />
                <circle
                  cx="10"
                  cy="10"
                  r="7.5"
                  class="stroke-brand-accent-text fill-none transition-all duration-300 ease-out"
                  stroke-width="2.5"
                  stroke-linecap="round"
                  stroke-dasharray="47.12"
                  stroke-dashoffset={47.12 * (1 - Math.min(1, Math.max(0, toast.task.progress)))}
                />
              </svg>
            {:else}
              <svg class="w-4 h-4 animate-spin text-brand-accent-text shrink-0" viewBox="0 0 20 20" fill="none">
                <circle
                  cx="10"
                  cy="10"
                  r="7.5"
                  stroke="currentColor"
                  stroke-width="2.5"
                  stroke-dasharray="24 24"
                  stroke-linecap="round"
                  class="opacity-75"
                />
              </svg>
            {/if}
          {:else if toast.task.status === "done"}
            <span class="relative inline-flex w-4 h-4 shrink-0 items-center justify-center">
              <span class="absolute inset-0 rounded-full anim-glow-ring"></span>
              <CheckCircle2 class="w-4 h-4 text-brand-accent-text anim-check-pop" />
            </span>
          {:else if toast.task.status === "failed"}
            <AlertTriangle class="w-4 h-4 shrink-0 text-brand-text-secondary" />
          {/if}
        </div>
      {:else if toast.variant === "error" || toast.variant === "warning"}
        <AlertTriangle class="w-4 h-4 shrink-0 translate-y-[calc((1lh-1rem)/2)]" />
      {:else if toast.variant === "success"}
        <span class="relative inline-flex w-4 h-4 shrink-0 items-center justify-center translate-y-[calc((1lh-1rem)/2)]">
          <span class="absolute inset-0 rounded-full anim-glow-ring"></span>
          <CheckCircle2 class="w-4 h-4 text-brand-accent-text anim-check-pop" />
        </span>
      {:else if toast.variant === "milestone"}
        <span class="relative inline-flex w-5 h-5 shrink-0 items-center justify-center translate-y-[calc((1lh-1.25rem)/2)]">
          <span class="absolute inset-0 rounded-full anim-gold-ring"></span>
          <CheckCheck class="w-5 h-5 text-brand-gold anim-milestone-bounce" />
        </span>
      {:else}
        <Info class="w-4 h-4 shrink-0 text-brand-accent-text translate-y-[calc((1lh-1rem)/2)]" />
      {/if}
      {#if toast.task}
        <div class="flex-1 min-w-0 flex flex-col justify-center">
          <div class="flex items-center justify-between gap-3">
            <span
              class="text-pretty transition-all duration-200"
              class:line-through={toast.task.status === "done"}
              class:text-brand-text-secondary={toast.task.status === "done"}
            >
              {toast.task.status === "done" ? (toast.task.taskName || toast.text) : toast.text}
            </span>
            {#if toast.task.status === "running" && toast.task.current !== undefined && toast.task.total !== undefined}
              <span class="text-xs font-mono text-brand-text-secondary shrink-0 select-none">
                {toast.task.current} / {toast.task.total}
              </span>
            {/if}
          </div>
          {#if toast.task.status === "running" && typeof toast.task.progress === "number"}
            <div class="w-full bg-brand-main rounded-full h-1 overflow-hidden border border-brand-border/40 mt-1.5">
              <div
                class="bg-brand-accent h-1 rounded-full transition-all duration-300 ease-out"
                style="width: {Math.min(100, Math.max(0, toast.task.progress * 100))}%"
              ></div>
            </div>
          {/if}
        </div>
      {:else if toast.url}
        <button
          type="button"
          onclick={() => openExternalUrl(toast.url!)}
          class="flex-1 text-left underline decoration-dotted underline-offset-2 hover:decoration-solid text-pretty"
        >
          {toast.text}
        </button>
      {:else}
        <span class="flex-1 text-pretty">{toast.text}</span>
      {/if}
      <div class="flex items-center gap-1 shrink-0 self-center">
        {#if toast.action}
          <Button
            onclick={() => toast.action!.onClick()}
            variant="primary"
            size="sm"
            class="shrink-0"
          >
            <RefreshCw class="w-3.5 h-3.5" />
            {toast.action.label}
          </Button>
        {/if}
        {#if toast.variant === "error"}
          <button
            type="button"
            onclick={() => copyToClipboard(toast.id, toast.text)}
            class="opacity-60 hover:opacity-100 transition-opacity p-0.5 rounded cursor-pointer"
            title={copiedToastId === toast.id
              ? i18n.t('toast.copied', {}, 'Copied to clipboard')
              : i18n.t('toast.copyError', {}, 'Copy error to clipboard')}
            aria-label={copiedToastId === toast.id
              ? i18n.t('toast.copied', {}, 'Copied to clipboard')
              : i18n.t('toast.copyError', {}, 'Copy error to clipboard')}
          >
            {#if copiedToastId === toast.id}
              <Check class="w-3.5 h-3.5 text-brand-accent-text" />
            {:else}
              <Clipboard class="w-3.5 h-3.5" />
            {/if}
          </button>
        {/if}
        <button
          type="button"
          onclick={() => {
            if (copiedToastId === toast.id) {
              if (copyTimeout) clearTimeout(copyTimeout);
              copiedToastId = null;
            }
            toastStore.dismiss(toast.id);
          }}
          class="opacity-60 hover:opacity-100 transition-opacity p-0.5 rounded cursor-pointer"
          title={i18n.t('toast.dismiss', {}, 'Dismiss notification')}
          aria-label={i18n.t('toast.dismiss', {}, 'Dismiss notification')}
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  {/each}
</div>
