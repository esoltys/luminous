<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import { Home, Info, Shield, MessageCircle, Bug } from "lucide-svelte";
</script>

<div class="space-y-6 max-w-4xl">
  <div class="bg-brand-sidebar border border-brand-border rounded-2xl p-6 flex flex-col md:flex-row items-center gap-6 shadow-md">
    <img src="/app-icon.svg" alt="Luminous Logo" class="w-20 h-20 shrink-0 drop-shadow-md" />
    <div class="space-y-2 text-center md:text-left flex-1 min-w-0">
      <div class="flex flex-wrap items-center justify-center md:justify-start gap-2.5">
        <h3 class="text-2xl font-bold text-brand-text-primary">{i18n.t('settings.aboutAppName', {}, 'Luminous Music Player')}</h3>
      </div>
      <p class="text-sm text-brand-text-secondary">{i18n.t('settings.aboutTagline')}</p>
      <p class="text-xs text-brand-text-secondary">
        {i18n.t('settings.aboutCreatedByPrefix', {}, 'Created by ')}
        <button
          onclick={() => openExternalUrl("https://esoltys.dev/")}
          class="text-brand-accent-text hover:text-brand-accent-text-hover font-bold hover:underline transition-colors"
          title="Eric Soltys — esoltys.dev"
        >
          Eric Soltys 🍁
        </button>
        {i18n.t('settings.aboutCreatedBySuffix', {}, ' in the BC Kootenays, Canada')}
      </p>
    </div>
  </div>

  <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
    <button
      onclick={() => openExternalUrl("https://esoltys.dev/luminous")}
      class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
    >
      <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
        <Home class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutHomepage')}</p>
        <p class="text-xs text-brand-text-secondary truncate">esoltys.dev/luminous</p>
      </div>
    </button>

    <button
      onclick={() => openExternalUrl("https://github.com/esoltys/luminous/blob/main/CREDITS.md")}
      class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
    >
      <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
        <Info class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutViewCredits')}</p>
        <p class="text-xs text-brand-text-secondary truncate">CREDITS.md</p>
      </div>
    </button>

    <button
      onclick={() => openExternalUrl("https://github.com/esoltys/luminous/blob/main/LICENSE")}
      class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
    >
      <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
        <Shield class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutViewLicense')}</p>
        <p class="text-xs text-brand-text-secondary truncate">{i18n.t('settings.aboutLicenseSubtitle', {}, 'MIT License')}</p>
      </div>
    </button>

    <button
      onclick={() => openExternalUrl("https://github.com/esoltys/luminous")}
      class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
    >
      <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
        <svg viewBox="0 0 24 24" fill="currentColor" class="w-5 h-5" aria-hidden="true">
          <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.238 1.838 1.238 1.07 1.833 2.809 1.304 3.495.997.108-.775.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/>
        </svg>
      </div>
      <div class="min-w-0">
        <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutGitHubRepo')}</p>
        <p class="text-xs text-brand-text-secondary truncate">github.com/esoltys/luminous</p>
      </div>
    </button>

    <button
      onclick={() => openExternalUrl("https://github.com/esoltys/luminous/discussions")}
      class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
    >
      <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
        <MessageCircle class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutDiscussions')}</p>
        <p class="text-xs text-brand-text-secondary truncate">github.com/esoltys/luminous/discussions</p>
      </div>
    </button>

    <button
      onclick={() => openExternalUrl("https://github.com/esoltys/luminous/issues")}
      class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
    >
      <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
        <Bug class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutIssues')}</p>
        <p class="text-xs text-brand-text-secondary truncate">github.com/esoltys/luminous/issues</p>
      </div>
    </button>
  </div>

  <div class="bg-brand-sidebar border border-brand-border rounded-2xl p-6 space-y-4">
    <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase border-b border-brand-border pb-2">
      {i18n.t('settings.aboutCoreTech')}
    </h4>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
      <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3.5 space-y-1">
        <p class="font-bold text-brand-text-primary">Rust & Tauri v2</p>
        <p class="text-brand-text-secondary leading-relaxed">{i18n.t('settings.aboutTechRust')}</p>
      </div>

      <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3.5 space-y-1">
        <p class="font-bold text-brand-text-primary">Svelte 5 & TypeScript</p>
        <p class="text-brand-text-secondary leading-relaxed">{i18n.t('settings.aboutTechSvelte')}</p>
      </div>
    </div>

    <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase border-b border-brand-border pb-2 pt-2">
      {i18n.t('settings.aboutAudioEngine')}
    </h4>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs">
      <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3">
        <p class="font-bold text-brand-text-primary">Symphonia & CPAL</p>
        <p class="text-xs text-brand-text-secondary mt-0.5">{i18n.t('settings.aboutTechSymphonia')}</p>
      </div>
      <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3">
        <p class="font-bold text-brand-text-primary">bs1770</p>
        <p class="text-xs text-brand-text-secondary mt-0.5">{i18n.t('settings.aboutTechBs1770')}</p>
      </div>
      <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3">
        <p class="font-bold text-brand-text-primary">rustfft & Lofty</p>
        <p class="text-xs text-brand-text-secondary mt-0.5">{i18n.t('settings.aboutTechRustfft')}</p>
      </div>
    </div>

    <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase border-b border-brand-border pb-2 pt-2">
      {i18n.t('settings.aboutInfluences')}
    </h4>

    <p class="text-xs text-brand-text-secondary leading-relaxed">
      {i18n.t('settings.aboutInfluencesText')}
    </p>
    <p class="text-xs text-brand-text-secondary leading-relaxed">
      {i18n.t('settings.aboutDedication')}
    </p>
  </div>
</div>
