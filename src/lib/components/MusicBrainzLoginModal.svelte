<script lang="ts">
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { musicbrainzStore } from "../stores/musicbrainz.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import {
    XIcon as X,
    ArrowUpRightIcon as ArrowUpRight,
    CircleNotchIcon as LoaderCircle,
    WarningIcon as AlertTriangle,
    CheckIcon as Check
  } from "phosphor-svelte";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let manualCode = $state("");

  $effect(() => {
    if (musicbrainzStore.isLoggedIn) {
      onClose();
    }
  });

  async function handleStartLogin() {
    try {
      await musicbrainzStore.startLogin(true);
    } catch {
      // Error handled in store
    }
  }

  async function handleSubmitCode() {
    const code = manualCode.trim();
    if (!code) return;
    try {
      await musicbrainzStore.submitAuthCode(code);
      onClose();
    } catch {
      // Error handled in store
    }
  }

  function handleClose() {
    musicbrainzStore.cancelLogin();
    onClose();
  }
</script>

<Modal onClose={handleClose} maxWidth="max-w-md">
  <div class="p-6 space-y-5">
    <!-- Header -->
    <div class="flex items-start justify-between gap-4">
      <div class="flex items-center gap-3">
        <div class="p-2.5 rounded-xl bg-[#ba478f]/15 border border-[#ba478f]/25 shrink-0 flex items-center justify-center">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 28" class="w-6 h-6">
            <polygon fill="#ba478f" points="12 0 0 7 0 21 12 28 12 0"/>
            <polygon fill="#eb743b" points="13 0 25 7 25 21 13 28 13 0"/>
          </svg>
        </div>
        <div>
          <h2 class="text-base font-bold text-brand-text-primary">
            {i18n.t('auth.modalTitle', {}, 'Sign in to MusicBrainz')}
          </h2>
          <p class="text-xs text-brand-text-secondary mt-0.5">
            {i18n.t('auth.modalSubtitle', {}, 'Connect your account to access collections and user stats')}
          </p>
        </div>
      </div>
      <button
        onclick={handleClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-1 rounded-lg hover:bg-white/5 transition-colors"
        title={i18n.t('common.close', {}, 'Close')}
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    {#if musicbrainzStore.authError}
      <div class="flex items-start gap-2.5 p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-xs">
        <AlertTriangle class="w-4 h-4 shrink-0 translate-y-[calc((1lh-1rem)/2)]" />
        <span class="flex-1 leading-relaxed">{musicbrainzStore.authError}</span>
      </div>
    {/if}

    {#if musicbrainzStore.isAuthorizing}
      <!-- Authorizing State -->
      <div class="space-y-4 py-2">
        <div class="flex flex-col items-center justify-center p-6 rounded-xl bg-white/[0.03] border border-brand-border text-center space-y-3">
          <LoaderCircle class="w-8 h-8 text-[#ba478f] animate-spin" />
          <div class="space-y-1">
            <p class="text-sm font-semibold text-brand-text-primary">
              {i18n.t('auth.waitingForBrowser', {}, 'Waiting for authorization in browser…')}
            </p>
            <p class="text-xs text-brand-text-secondary max-w-xs">
              {i18n.t('auth.browserPromptDesc', {}, 'Log in to MusicBrainz and approve access. This window will automatically update.')}
            </p>
          </div>
          <Button variant="secondary" size="sm" onclick={() => musicbrainzStore.cancelLogin()}>
            {i18n.t('common.cancel', {}, 'Cancel')}
          </Button>
        </div>

        <!-- Manual verification code fallback -->
        <div class="space-y-2 pt-2 border-t border-brand-border/60">
          <label for="mb-auth-code" class="text-xs font-medium text-brand-text-secondary block">
            {i18n.t('auth.enterCodeManually', {}, 'Or paste verification code:')}
          </label>
          <div class="flex items-center gap-2">
            <Input
              id="mb-auth-code"
              placeholder={i18n.t('auth.codePlaceholder', {}, 'Paste code here…')}
              bind:value={manualCode}
              class="flex-1 text-xs"
            />
            <Button
              variant="primary"
              size="sm"
              disabled={!manualCode.trim() || musicbrainzStore.isLoading}
              onclick={handleSubmitCode}
            >
              {#if musicbrainzStore.isLoading}
                <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
              {:else}
                <Check class="w-3.5 h-3.5" />
              {/if}
              {i18n.t('auth.submitCode', {}, 'Submit')}
            </Button>
          </div>
        </div>
      </div>
    {:else}
      <!-- Initial State -->
      <div class="space-y-4">
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t('auth.loginDescription', {}, 'Signing in links your MusicBrainz editor profile with Luminous, allowing you to view your collections and stats directly from the player.')}
        </p>

        <Button
          variant="primary"
          class="w-full justify-center py-2.5 font-medium flex items-center gap-2.5 bg-gradient-to-r from-[#ba478f] to-[#eb743b] hover:opacity-95 text-white border-0 shadow-lg shadow-[#ba478f]/20"
          onclick={handleStartLogin}
        >
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 28" class="w-4 h-4 fill-current">
            <polygon points="12 0 0 7 0 21 12 28 12 0"/>
            <polygon points="13 0 25 7 25 21 13 28 13 0"/>
          </svg>
          {i18n.t('auth.signInWithMb', {}, 'Sign in with MusicBrainz')}
        </Button>

        <div class="flex items-center justify-center gap-1.5 text-xs text-brand-text-secondary pt-1">
          <span>{i18n.t('auth.noAccountPrompt', {}, "Don't have an account?")}</span>
          <button
            type="button"
            onclick={() => openExternalUrl("https://musicbrainz.org/register")}
            class="text-brand-text-primary hover:text-brand-accent-text-hover hover:underline underline underline-offset-2 font-semibold inline-flex items-center gap-0.5 transition-colors cursor-pointer"
          >
            {i18n.t('auth.createAccountLink', {}, 'Create an account')}
            <ArrowUpRight class="w-3 h-3" />
          </button>
        </div>
      </div>
    {/if}
  </div>
</Modal>
