<script lang="ts">
  import { LoaderCircle } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  let isLoading = $state(true);
</script>

<div class="relative flex flex-col h-full overflow-hidden bg-brand-main">
  {#if isLoading}
    <div class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-brand-main">
      <LoaderCircle class="w-8 h-8 animate-spin text-brand-accent-text" />
      <span class="text-xs text-brand-text-secondary/60 font-medium">{i18n.t('help.loading', {}, "Loading user guide...")}</span>
    </div>
  {/if}
  <iframe
    src="/luminous-user-guide.html"
    title={i18n.t('sidebar.help')}
    class="flex-1 w-full h-full border-0 transition-opacity duration-150 {isLoading ? 'opacity-0' : 'opacity-100'}"
    onload={() => { isLoading = false; }}
  ></iframe>
</div>
