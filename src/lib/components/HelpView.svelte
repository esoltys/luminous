<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { walkthroughStore } from "../stores/walkthrough.svelte";
  import { CompassIcon as Compass } from "phosphor-svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import Button from "./Button.svelte";

  let isLoading = $state(true);
</script>

<div class="relative flex flex-col h-full overflow-hidden bg-brand-main">
  {#if isLoading}
    <div class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-brand-main">
      <LoadingSpinner label={i18n.t('help.loading', {}, "Loading user guide...")} />
    </div>
  {/if}
  <div class="absolute top-4 right-4 z-20">
    <Button onclick={() => walkthroughStore.start()} variant="secondary" size="sm">
      <Compass class="w-3.5 h-3.5" />
      {i18n.t('walkthrough.restartTour', {}, 'Restart Feature Walkthrough')}
    </Button>
  </div>
  <iframe
    src="/luminous-user-guide-{i18n.currentLocale.toUpperCase()}.html"
    title={i18n.t('sidebar.help')}
    class="flex-1 w-full h-full border-0 transition-opacity duration-150 {isLoading ? 'opacity-0' : 'opacity-100'}"
    onload={(e) => {
      isLoading = false;
      (e.currentTarget as HTMLIFrameElement).contentWindow?.scrollTo(0, 0);
    }}
  ></iframe>
</div>
