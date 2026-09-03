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
    CheckIcon as Check
  } from "phosphor-svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";

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
          <CheckCheck class="w-5 h-5 text-brand-gold anim-milestone-bounce" />
        </span>
      {:else}
        <Info class="w-4 h-4 shrink-0 text-brand-accent-text" />
      {/if}
      {#if toast.url}
        <button
          type="button"
          onclick={() => openExternalUrl(toast.url!)}
          class="flex-1 text-left underline decoration-dotted underline-offset-2 hover:decoration-solid"
        >
          {toast.text}
        </button>
      {:else}
        <span class="flex-1">{toast.text}</span>
      {/if}
      <div class="flex items-center gap-1 shrink-0">
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
              <Check class="w-3.5 h-3.5 text-emerald-400" />
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
