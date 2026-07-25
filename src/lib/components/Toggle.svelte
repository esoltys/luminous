<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    checked: boolean;
    onchange: (checked: boolean) => void;
    label: string;
    id?: string;
    disabled?: boolean;
    showOnOffLabel?: boolean;
  }

  let { checked, onchange, label, id, disabled = false, showOnOffLabel = true }: Props = $props();
</script>

<div class="flex items-center gap-2 shrink-0">
  {#if showOnOffLabel}
    <span class="text-xs font-medium text-brand-text-secondary text-right whitespace-nowrap min-w-[4.5rem]">
      {checked ? i18n.t('common.on') : i18n.t('common.off')}
    </span>
  {/if}
  <button
    {id}
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    {disabled}
    onclick={() => onchange(!checked)}
    class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed {checked ? 'bg-brand-accent' : 'bg-brand-border'}"
  >
    <span class="inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform {checked ? 'translate-x-6' : 'translate-x-1'}"></span>
  </button>
</div>
