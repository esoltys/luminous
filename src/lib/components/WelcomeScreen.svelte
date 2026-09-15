<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { i18n } from "../stores/i18n.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import Button from "./Button.svelte";

  let { onGetStarted }: { onGetStarted: () => void } = $props();
</script>

<!-- The app's one true "welcome" moment — shown once, ever, on the very
     first launch, ahead of the rest of the shell. Dismissing it (Get
     Started) hands off straight into the guided tour. -->
<div
  class="fixed inset-0 z-[120] flex items-center justify-center bg-brand-main/70 backdrop-blur-md select-none"
  transition:fade={{ duration: 200 }}
>
  <div
    class="flex flex-col items-center text-center max-w-sm mx-4 p-8 bg-brand-sidebar border border-brand-border rounded-2xl shadow-2xl"
    transition:scale={{ duration: 200, start: 0.97 }}
  >
    <img src="/app-icon.svg" alt="" class="w-16 h-16 mb-5 drop-shadow-md" />
    <h1 class="text-xl font-bold text-brand-text-primary mb-2 text-balance">{i18n.t('welcome.title')}</h1>
    <p class="text-sm text-brand-text-secondary mb-6 leading-relaxed text-pretty">{i18n.t('welcome.subtitle')}</p>
    <Button onclick={onGetStarted} variant="primary" size="md" class="w-full">
      {i18n.t('welcome.getStarted')}
    </Button>
    <p class="text-xs text-brand-text-secondary/70 mt-5 leading-relaxed text-balance">
      {i18n.t('welcome.legalPrefix')}
      <button
        onclick={() => openExternalUrl('https://github.com/esoltys/luminous/blob/main/TERMS.md')}
        class="text-brand-text-secondary underline hover:text-brand-accent-text transition-colors"
      >{i18n.t('welcome.termsOfService')}</button>
      {i18n.t('welcome.and')}
      <button
        onclick={() => openExternalUrl('https://github.com/esoltys/luminous/blob/main/PRIVACY.md')}
        class="text-brand-text-secondary underline hover:text-brand-accent-text transition-colors"
      >{i18n.t('welcome.privacyPolicy')}</button>.
    </p>
  </div>
</div>
