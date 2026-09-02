<script lang="ts">
  import type { Component } from "svelte";
  import { FunnelXIcon as FilterX } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import Button from "./Button.svelte";

  interface Props {
    icon: Component<{ class?: string }>;
    title: string;
    matchQueryText: string;
    query: string;
    onReset: () => void;
  }

  let { icon: Icon, title, matchQueryText, query, onReset }: Props = $props();
</script>

<div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
  <Icon class="w-12 h-12 text-brand-accent-text/40 mb-3 animate-pulse" />
  <h3 class="text-base font-semibold text-brand-text-primary mb-1">{title}</h3>
  <p class="text-xs text-brand-text-secondary mb-4">
    {matchQueryText} <code class="bg-brand-sidebar px-1 py-0.5 rounded font-mono text-brand-accent-text">{query}</code>
  </p>
  <Button onclick={onReset} variant="primary" size="sm">
    <FilterX class="w-3.5 h-3.5" />
    {i18n.t('collection.resetSearchFilters')}
  </Button>
</div>
