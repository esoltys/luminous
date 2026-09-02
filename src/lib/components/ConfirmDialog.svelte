<script lang="ts">
  import { WarningIcon as AlertTriangle, XIcon as X } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";

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
    <button onclick={onCancel} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-6">
    <p class="text-sm text-brand-text-secondary">{message}</p>
  </div>

  <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-brand-border bg-brand-main">
    <Button onclick={onCancel} variant="secondary" size="sm">
      {cancelLabel}
    </Button>
    <Button onclick={onConfirm} variant={danger ? "destructive" : "primary"} size="sm">
      {confirmLabel}
    </Button>
  </div>
</Modal>
