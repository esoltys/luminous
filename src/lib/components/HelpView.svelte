<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { walkthroughStore } from "../stores/walkthrough.svelte";
  import { CompassIcon as Compass } from "phosphor-svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import Button from "./Button.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";

  let isLoading = $state(true);

  /**
   * Open the guide's web links in the system browser. Inside the iframe a
   * link would otherwise navigate the guide away, or — with target=_blank —
   * ask for a new webview window, which the app doesn't allow, so nothing
   * happens. In-page anchors and the guide's own files are left alone.
   */
  function openWebLinksExternally(doc: Document) {
    doc.addEventListener("click", (e) => {
      const link = (e.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
      if (!link || !/^https?:$/.test(link.protocol) || link.origin === doc.location.origin) return;
      e.preventDefault();
      openExternalUrl(link.href);
    });
  }
</script>

<div class="relative flex flex-col h-full overflow-hidden bg-brand-main">
  {#if isLoading}
    <div class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-brand-main">
      <LoadingSpinner label={i18n.t('help.loading', {}, "Loading user guide...")} />
    </div>
  {/if}
  <div class="absolute top-4 right-6 z-20">
    <Button onclick={() => walkthroughStore.start()} variant="secondary" size="sm">
      <Compass class="w-3.5 h-3.5" />
      {i18n.t('walkthrough.restartTour', {}, 'Restart Feature Walkthrough')}
    </Button>
  </div>
  <iframe
    src="/luminous-user-guide-{i18n.manualLanguage}.html"
    title={i18n.t('sidebar.help')}
    class="flex-1 w-full h-full border-0 transition-opacity duration-150 {isLoading ? 'opacity-0' : 'opacity-100'}"
    onload={(e) => {
      isLoading = false;
      const frame = e.currentTarget as HTMLIFrameElement;
      frame.contentWindow?.scrollTo(0, 0);
      if (frame.contentDocument) openWebLinksExternally(frame.contentDocument);
    }}
  ></iframe>
</div>
