<script lang="ts">
  import { untrack } from "svelte";
  import type { WebDavServer } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { PLAYER_DOCK_CLEARANCE_PX } from "../constants";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import {
    CloudIcon,
    XIcon as X,
    CheckCircleIcon as CheckCircle,
    WarningCircleIcon as WarningCircle,
    CircleNotchIcon as LoaderCircle,
  } from "phosphor-svelte";

  interface Props {
    server: WebDavServer | null;
    onClose: () => void;
    onSaved: (server: WebDavServer) => void;
  }

  let { server, onClose, onSaved }: Props = $props();

  let name = $state(untrack(() => server?.name ?? ""));
  let url = $state(untrack(() => server?.url ?? ""));
  let username = $state(untrack(() => server?.username ?? ""));
  let password = $state("");
  let remotePath = $state(untrack(() => server?.remotePath ?? "/"));
  let enabled = $state(untrack(() => server?.enabled ?? true));

  let testing = $state(false);
  let testSuccess = $state<boolean | null>(null);
  let testError = $state<string | null>(null);
  let saving = $state(false);

  const dockClearance = $derived(
    playerStore.currentSong ? PLAYER_DOCK_CLEARANCE_PX : 0
  );

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  async function handleTestConnection() {
    if (!url.trim()) return;
    testing = true;
    testSuccess = null;
    testError = null;

    try {
      await invoke("test_webdav_connection", {
        url: url.trim(),
        username: username.trim() || null,
        password: password ? password : null,
      });
      testSuccess = true;
    } catch (err: any) {
      testSuccess = false;
      testError = typeof err === "string" ? err : err?.message || String(err);
    } finally {
      testing = false;
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (saving || !name.trim() || !url.trim()) return;
    saving = true;

    try {
      const saved = await invoke<WebDavServer>("save_webdav_server", {
        id: server?.id ?? null,
        name: name.trim(),
        url: url.trim(),
        username: username.trim() || null,
        password: password ? password : null,
        remotePath: remotePath.trim() || "/",
        enabled,
      });
      onSaved(saved);
      onClose();
    } catch (err) {
      console.error("Failed to save WebDAV server:", err);
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
            {server ? i18n.t("settings.editWebdavServer") : i18n.t("settings.addWebdavServer")}
          </h2>
          <p class="text-xs text-brand-text-secondary/70 truncate">{i18n.t("settings.webdavSubtitle")}</p>
        </div>
      </div>
      <button
        type="button"
        onclick={onClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-1.5 rounded-lg hover:bg-brand-main/80 transition-colors"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <form onsubmit={handleSubmit} class="p-6 flex-1 overflow-y-auto flex flex-col gap-4">
      <div class="space-y-1.5">
        <label for="webdav-name" class="block text-xs font-semibold text-brand-text-secondary">
          {i18n.t("settings.webdavName")}
        </label>
        <Input
          id="webdav-name"
          bind:value={name}
          placeholder={i18n.t("settings.webdavNamePlaceholder")}
          required
        />
      </div>

      <div class="space-y-1.5">
        <label for="webdav-url" class="block text-xs font-semibold text-brand-text-secondary">
          {i18n.t("settings.webdavUrl")}
        </label>
        <Input
          id="webdav-url"
          bind:value={url}
          placeholder={i18n.t("settings.webdavUrlPlaceholder")}
          required
        />
      </div>

      <div class="grid grid-cols-2 gap-3">
        <div class="space-y-1.5">
          <label for="webdav-username" class="block text-xs font-semibold text-brand-text-secondary">
            {i18n.t("settings.webdavUsername")}
          </label>
          <Input
            id="webdav-username"
            bind:value={username}
            placeholder={i18n.t("settings.webdavUsernamePlaceholder")}
          />
        </div>

        <div class="space-y-1.5">
          <label for="webdav-password" class="block text-xs font-semibold text-brand-text-secondary">
            {i18n.t("settings.webdavPassword")}
          </label>
          <Input
            id="webdav-password"
            type="password"
            bind:value={password}
            placeholder={i18n.t("settings.webdavPasswordPlaceholder")}
          />
        </div>
      </div>

      <div class="space-y-1.5">
        <label for="webdav-remote-path" class="block text-xs font-semibold text-brand-text-secondary">
          {i18n.t("settings.webdavRemotePath")}
        </label>
        <Input
          id="webdav-remote-path"
          bind:value={remotePath}
          placeholder="/Music"
        />
      </div>

      {#if testSuccess === true}
        <div class="flex items-center gap-2 p-3 bg-emerald-500/10 border border-emerald-500/30 rounded-xl text-xs text-emerald-400">
          <CheckCircle class="w-4 h-4 shrink-0" />
          <span>{i18n.t("settings.webdavTestSuccess")}</span>
        </div>
      {:else if testSuccess === false}
        <div class="flex items-center gap-2 p-3 bg-red-500/10 border border-red-500/30 rounded-xl text-xs text-red-400">
          <WarningCircle class="w-4 h-4 shrink-0" />
          <span>{i18n.t("settings.webdavTestFailed", { error: testError || "" })}</span>
        </div>
      {/if}

      <div class="pt-2 flex items-center justify-between gap-3 border-t border-brand-border/60">
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={testing || !url.trim()}
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
          <Button type="submit" variant="primary" size="sm" disabled={saving || !name.trim() || !url.trim()}>
            {server ? i18n.t("settings.saveWebdavServer") : i18n.t("settings.addWebdavServer")}
          </Button>
        </div>
      </div>
    </form>
  </div>
</div>
