<script lang="ts">
  import { untrack } from "svelte";
  import type { SubsonicAuthMode, SubsonicServer, SubsonicServerProbe } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { PLAYER_DOCK_CLEARANCE_PX } from "../constants";
  import { BADGE_ICON_CHOICES, BADGE_COLOR_CHOICES } from "../badgeChoices";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import LibraryBadge from "./LibraryBadge.svelte";
  import ColorPicker from "./ColorPicker.svelte";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import {
    CloudIcon,
    XIcon as X,
    CheckCircleIcon as CheckCircle,
    WarningCircleIcon as WarningCircle,
    CircleNotchIcon as LoaderCircle,
  } from "phosphor-svelte";

  interface Props {
    server: SubsonicServer | null;
    onClose: () => void;
    onSaved: (server: SubsonicServer) => void;
  }

  let { server, onClose, onSaved }: Props = $props();

  let name = $state(untrack(() => server?.name ?? ""));
  let url = $state(untrack(() => server?.url ?? ""));
  let username = $state(untrack(() => server?.username ?? ""));
  let authMode = $state<SubsonicAuthMode>(untrack(() => server?.authMode ?? "token"));
  // Never pre-filled: a blank password/API key on edit keeps the stored one.
  let password = $state("");
  // Whether the server at `url` offers API-key sign-in (asked without credentials).
  let apiKeySupported = $state(false);
  let enabled = $state(untrack(() => server?.enabled ?? true));
  let autoSyncEnabled = $state(untrack(() => server?.autoSyncEnabled ?? false));
  let syncIntervalMinutes = $state(untrack(() => server?.syncIntervalMinutes ?? 60));
  let reportPlays = $state(untrack(() => server?.reportPlays ?? true));
  let selectedIcon = $state(untrack(() => server?.icon ?? "cloud"));
  let selectedColor = $state<string | null>(untrack(() => server?.color ?? null));

  let testing = $state(false);
  let testProbe = $state<SubsonicServerProbe | null>(null);
  let testError = $state<string | null>(null);
  // Subsonic API error code of the last failed test (41 = token sign-in unsupported).
  let testErrorCode = $state<number | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  let isApiKey = $derived(authMode === "apiKey");
  // The stored secret only carries over between token and password sign-in;
  // switching to or from an API key needs a new one.
  let storedSecretFits = $derived(
    !!server && (server.authMode === "apiKey") === isApiKey
  );
  // A new server can't be tested or saved without a secret; an existing one
  // falls back to its stored one.
  let hasCredentials = $derived(
    !!url.trim() && (isApiKey || !!username.trim()) && (!!password || storedSecretFits)
  );
  let canSave = $derived(!!name.trim() && hasCredentials);
  let showApiKeyOption = $derived(apiKeySupported || isApiKey);
  let showHttpWarning = $derived(
    authMode === "password" && url.trim().toLowerCase().startsWith("http://")
  );
  let suggestPassword = $derived(authMode === "token" && testErrorCode === 41);

  // Ask the server which sign-in methods it offers once the URL settles.
  $effect(() => {
    const target = url.trim();
    if (!/^https?:\/\/\S+/i.test(target)) {
      apiKeySupported = false;
      return;
    }
    let stale = false;
    const timer = setTimeout(async () => {
      try {
        const support = await invoke<{ apiKey: boolean }>("get_subsonic_auth_support", { url: target });
        if (!stale) apiKeySupported = support.apiKey;
      } catch {
        if (!stale) apiKeySupported = false;
      }
    }, 400);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  });

  function setAuthMode(mode: SubsonicAuthMode) {
    authMode = mode;
    testProbe = null;
    testError = null;
    testErrorCode = null;
  }

  let previewSource = $derived({
    path: url.trim(),
    nickname: name.trim() || null,
    icon: selectedIcon,
    color: selectedColor,
  });

  let testSuccessText = $derived.by(() => {
    if (!testProbe) return "";
    const label = [testProbe.serverType, testProbe.serverVersion].filter(Boolean).join(" ");
    return label
      ? i18n.t("settings.subsonicTestSuccess", { server: label })
      : i18n.t("settings.subsonicTestSuccessUnknown");
  });

  const dockClearance = $derived(
    playerStore.currentSong ? PLAYER_DOCK_CLEARANCE_PX : 0
  );

  function errorText(err: unknown): string {
    return typeof err === "string" ? err : (err as any)?.message || String(err);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  async function handleTestConnection() {
    if (!hasCredentials) return;
    testing = true;
    testProbe = null;
    testError = null;
    testErrorCode = null;

    try {
      testProbe = await invoke<SubsonicServerProbe>("test_subsonic_connection", {
        url: url.trim(),
        username: isApiKey ? "" : username.trim(),
        password: password ? password : null,
        authMode,
        id: server?.id ?? null,
      });
    } catch (err) {
      testError = errorText(err);
      testErrorCode = typeof (err as any)?.code === "number" ? (err as any).code : null;
    } finally {
      testing = false;
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (saving || !canSave) return;
    saving = true;
    saveError = null;

    try {
      const saved = await invoke<SubsonicServer>("save_subsonic_server", {
        input: {
          id: server?.id ?? null,
          name: name.trim(),
          url: url.trim(),
          username: isApiKey ? "" : username.trim(),
          password: password ? password : null,
          authMode,
          enabled,
          nickname: name.trim() || null,
          icon: selectedIcon,
          color: selectedColor,
          autoSyncEnabled,
          syncIntervalMinutes: Math.max(1, Math.round(syncIntervalMinutes) || 60),
          reportPlays,
        },
      });
      onSaved(saved);
      onClose();
    } catch (err) {
      saveError = errorText(err);
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
  style="padding-bottom: {dockClearance + 16}px"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
>
  <div
    class="bg-brand-sidebar border border-brand-border rounded-2xl w-full max-w-lg shadow-2xl overflow-hidden flex flex-col animate-in fade-in zoom-in-95 duration-150"
    style="max-height: min(90vh, calc(100vh - {dockClearance + 32}px))"
  >
    <div class="flex items-center justify-between px-6 py-4 border-b border-brand-border/60 bg-brand-main/50">
      <div class="flex items-center gap-2.5 min-w-0">
        <div class="p-2 rounded-xl bg-brand-accent/20 text-brand-accent-text shrink-0">
          <CloudIcon class="w-5 h-5" />
        </div>
        <div class="min-w-0">
          <h2 class="text-base font-bold text-brand-text-primary truncate">
            {server ? i18n.t("settings.editSubsonicServer") : i18n.t("settings.addSubsonicServer")}
          </h2>
          <p class="text-xs text-brand-text-secondary/70 truncate">{i18n.t("settings.subsonicSubtitle")}</p>
        </div>
      </div>
      <button
        type="button"
        onclick={onClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-1.5 rounded-lg hover:bg-brand-main/80 transition-colors"
        aria-label={i18n.t("settings.cancel")}
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <form onsubmit={handleSubmit} class="p-6 flex-1 overflow-y-auto flex flex-col gap-4">
      <div class="grid grid-cols-2 gap-3">
        <div class="space-y-1.5">
          <label for="subsonic-name" class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
            {i18n.t("settings.subsonicName")}
          </label>
          <Input
            id="subsonic-name"
            bind:value={name}
            placeholder={i18n.t("settings.subsonicNamePlaceholder")}
            required
          />
        </div>

        <div class="space-y-1.5">
          <label for="subsonic-url" class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
            {i18n.t("settings.subsonicUrl")}
          </label>
          <Input
            id="subsonic-url"
            bind:value={url}
            placeholder={i18n.t("settings.subsonicUrlPlaceholder")}
            required
          />
        </div>
      </div>

      <div class="space-y-1.5">
        <label for="subsonic-auth-mode" class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
          {i18n.t("settings.subsonicAuthMode")}
        </label>
        <Select
          id="subsonic-auth-mode"
          value={authMode}
          onchange={(e) => setAuthMode(e.currentTarget.value as SubsonicAuthMode)}
          class="bg-brand-main border border-brand-border rounded-xl px-3.5 py-2 text-sm font-medium text-brand-text-primary outline-none focus:border-brand-accent w-full pr-8"
        >
          <option value="token">{i18n.t("settings.subsonicAuthToken")}</option>
          <option value="password">{i18n.t("settings.subsonicAuthPassword")}</option>
          {#if showApiKeyOption}
            <option value="apiKey">{i18n.t("settings.subsonicAuthApiKey")}</option>
          {/if}
        </Select>
        <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t("settings.subsonicAuthModeHint")}</p>
      </div>

      {#if showHttpWarning}
        <div class="flex items-start gap-2 p-3 bg-brand-main/60 border border-brand-border rounded-xl text-xs text-brand-text-primary" data-testid="subsonic-http-warning">
          <WarningCircle class="w-4 h-4 shrink-0 translate-y-[calc((1lh-1rem)/2)] text-brand-text-secondary" />
          <span>{i18n.t("settings.subsonicAuthHttpWarning")}</span>
        </div>
      {/if}

      <div class="grid {isApiKey ? 'grid-cols-1' : 'grid-cols-2'} gap-3">
        {#if !isApiKey}
          <div class="space-y-1.5">
            <label for="subsonic-username" class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
              {i18n.t("settings.subsonicUsername")}
            </label>
            <Input
              id="subsonic-username"
              bind:value={username}
              placeholder={i18n.t("settings.subsonicUsernamePlaceholder")}
              required
            />
          </div>
        {/if}

        <div class="space-y-1.5">
          <label for="subsonic-password" class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
            {isApiKey ? i18n.t("settings.subsonicApiKey") : i18n.t("settings.subsonicPassword")}
          </label>
          <Input
            id="subsonic-password"
            type="password"
            bind:value={password}
            placeholder={storedSecretFits
              ? (isApiKey
                ? i18n.t("settings.subsonicApiKeyKeepPlaceholder")
                : i18n.t("settings.subsonicPasswordKeepPlaceholder"))
              : (isApiKey
                ? i18n.t("settings.subsonicApiKeyPlaceholder")
                : i18n.t("settings.subsonicPasswordPlaceholder"))}
            required={!storedSecretFits}
          />
        </div>
      </div>

      <!-- Auto-Sync -->
      <div class="bg-brand-main/40 border border-brand-border/50 rounded-xl p-4 space-y-3">
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5 min-w-0">
            <span class="text-sm font-medium text-brand-text-primary">{i18n.t("settings.webdavAutoSyncLabel")}</span>
            <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t("settings.webdavAutoSyncHint")}</p>
          </div>
          <Toggle
            checked={autoSyncEnabled}
            onchange={(v) => { autoSyncEnabled = v; }}
            label={i18n.t("settings.webdavAutoSyncLabel")}
          />
        </div>
        {#if autoSyncEnabled}
          <div class="space-y-1.5">
            <label for="subsonic-sync-interval" class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
              {i18n.t("settings.webdavSyncIntervalLabel")}
            </label>
            <Input
              id="subsonic-sync-interval"
              type="number"
              bind:value={syncIntervalMinutes}
            />
          </div>
        {/if}
      </div>

      <!-- Play reporting -->
      <div class="bg-brand-main/40 border border-brand-border/50 rounded-xl p-4 flex items-center justify-between gap-4">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t("settings.subsonicReportPlaysLabel")}</span>
          <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t("settings.subsonicReportPlaysHint")}</p>
        </div>
        <Toggle
          checked={reportPlays}
          onchange={(v) => { reportPlays = v; }}
          label={i18n.t("settings.subsonicReportPlaysLabel")}
        />
      </div>

      <!-- Live Preview -->
      <div class="bg-brand-main/40 border border-brand-border/50 rounded-xl p-4 flex items-center justify-between gap-4">
        <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
          {i18n.t("settings.folderPreview")}
        </span>
        <div>
          <LibraryBadge directory={previewSource} size="md" />
        </div>
      </div>

      <!-- Icon Selection -->
      <div>
        <span class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider mb-2">
          {i18n.t("settings.folderIcon")}
        </span>
        <div class="grid grid-cols-5 gap-2">
          {#each BADGE_ICON_CHOICES as choice}
            {@const Icon = choice.icon}
            {@const isSelected = selectedIcon === choice.id}
            <button
              type="button"
              onclick={() => { selectedIcon = choice.id; }}
              class="flex flex-col items-center justify-center p-2.5 rounded-xl border transition-all duration-150 gap-1
                {isSelected
                  ? 'bg-brand-accent/20 border-brand-accent text-brand-accent-text ring-1 ring-brand-accent'
                  : 'bg-brand-main/40 border-brand-border/60 text-brand-text-secondary hover:text-brand-text-primary hover:border-brand-border'}"
              title={i18n.t(choice.label)}
            >
              <Icon class="w-5 h-5" />
              <span class="text-[10px] font-medium truncate max-w-full">{i18n.t(choice.label)}</span>
            </button>
          {/each}
        </div>
      </div>

      <!-- Color Selection -->
      <div>
        <span class="block font-medium text-xs text-brand-text-secondary uppercase tracking-wider mb-2">
          {i18n.t("settings.folderColor")}
        </span>
        <ColorPicker choices={BADGE_COLOR_CHOICES} value={selectedColor} onChange={(v) => { selectedColor = v; }} />
      </div>

      {#if testProbe}
        <div class="flex items-start gap-2 p-3 bg-brand-accent/10 border border-brand-accent/30 rounded-xl text-xs text-brand-accent-text" data-testid="subsonic-test-success">
          <CheckCircle class="w-4 h-4 shrink-0 translate-y-[calc((1lh-1rem)/2)]" />
          <span>{testSuccessText}</span>
        </div>
      {:else if testError !== null}
        <div class="flex items-start gap-2 p-3 bg-brand-main/60 border border-brand-border rounded-xl text-xs text-brand-text-primary" data-testid="subsonic-test-failed">
          <WarningCircle class="w-4 h-4 shrink-0 translate-y-[calc((1lh-1rem)/2)] text-brand-text-secondary" />
          <div class="flex flex-col items-start gap-2">
            <span>{i18n.t("settings.webdavTestFailed", { error: testError })}</span>
            {#if suggestPassword}
              <button
                type="button"
                data-testid="subsonic-use-password"
                onclick={() => setAuthMode("password")}
                class="px-3 py-1 rounded-lg border border-brand-border text-brand-text-primary hover:border-brand-accent transition-colors"
              >
                {i18n.t("settings.subsonicUsePasswordAuth")}
              </button>
            {/if}
          </div>
        </div>
      {/if}

      {#if saveError !== null}
        <div class="flex items-start gap-2 p-3 bg-brand-main/60 border border-brand-border rounded-xl text-xs text-brand-text-primary" data-testid="subsonic-save-failed">
          <WarningCircle class="w-4 h-4 shrink-0 translate-y-[calc((1lh-1rem)/2)] text-brand-text-secondary" />
          <span>{i18n.t("settings.subsonicSaveFailed", { error: saveError })}</span>
        </div>
      {/if}

      <div class="pt-2 flex items-center justify-between gap-3 border-t border-brand-border/60">
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={testing || !hasCredentials}
          onclick={handleTestConnection}
        >
          {#if testing}
            <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
            {i18n.t("settings.webdavTesting")}
          {:else}
            {i18n.t("settings.webdavTestBtn")}
          {/if}
        </Button>

        <div class="flex items-center gap-2">
          <Button type="button" variant="secondary" size="sm" onclick={onClose}>
            {i18n.t("settings.cancel")}
          </Button>
          <Button type="submit" variant="primary" size="sm" disabled={saving || !canSave}>
            {server ? i18n.t("settings.saveWebdavServer") : i18n.t("settings.addSubsonicServer")}
          </Button>
        </div>
      </div>
    </form>
  </div>
</div>
