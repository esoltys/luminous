<script lang="ts">
  import type { Snippet } from "svelte";
  import { portal } from "../utils/portal";

  interface Props {
    onClose: () => void;
    onKeydown?: (e: KeyboardEvent) => void;
    maxWidth?: string;
    class?: string;
    closeOnBackdropClick?: boolean;
    ariaLabelledby?: string;
    children: Snippet;
  }

  let {
    onClose,
    onKeydown,
    maxWidth = "max-w-lg",
    class: className = "",
    closeOnBackdropClick = false,
    ariaLabelledby,
    children,
  }: Props = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
    onKeydown?.(e);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  use:portal
  role="presentation"
  class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-xs select-none"
  onclick={closeOnBackdropClick ? onClose : undefined}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    role="dialog"
    aria-modal="true"
    aria-labelledby={ariaLabelledby}
    tabindex="-1"
    class="bg-brand-sidebar border border-brand-border rounded-2xl w-full {maxWidth} overflow-hidden shadow-2xl flex flex-col text-brand-text-primary {className}"
    onclick={(e) => e.stopPropagation()}
  >
    {@render children()}
  </div>
</div>
