<script lang="ts">
  import { AlertTriangle, X } from "lucide-svelte";
  import Modal from "./Modal.svelte";

  let {
    title,
    message,
    confirmLabel,
    cancelLabel,
    danger = true,
    onConfirm,
    onCancel,
  }: {
    title: string;
    message: string;
    confirmLabel: string;
    cancelLabel: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      const target = e.target as HTMLElement;
      if (target.tagName === "BUTTON") return;
      e.preventDefault();
      onConfirm();
    }
  }
</script>

<Modal onClose={onCancel} onKeydown={handleKeydown} maxWidth="max-w-sm">
  <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
    <div class="flex items-center gap-2">
      <AlertTriangle class="w-4 h-4 {danger ? 'text-red-400' : 'text-brand-accent-text'}" />
      <h3 class="text-sm font-bold">{title}</h3>
    </div>
    <button onclick={onCancel} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer">
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-6">
    <p class="text-sm text-brand-text-secondary">{message}</p>
  </div>

  <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-brand-border bg-brand-main">
    <button
      onclick={onCancel}
      class="px-4 py-2 text-xs font-semibold rounded-md border border-brand-border text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-sidebar transition-colors cursor-pointer"
    >
      {cancelLabel}
    </button>
    <button
      onclick={onConfirm}
      class="px-4 py-2 text-xs font-semibold rounded-md transition-colors cursor-pointer {danger ? 'bg-red-500/10 hover:bg-red-500/20 border border-red-500/30 text-red-400 hover:text-red-300' : 'bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast'}"
    >
      {confirmLabel}
    </button>
  </div>
</Modal>
