<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import type { MetadataRejection } from "../types";
  import {
    XIcon as X,
    ProhibitIcon as Prohibit,
    TrashIcon as Trash,
    ShieldSlashIcon as ShieldSlash,
  } from "phosphor-svelte";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let rejections = $state<MetadataRejection[]>([]);
  let isLoading = $state(true);
  let isUnblocking = $state<number | null>(null);

  async function loadRejections() {
    isLoading = true;
    try {
      const res = await invoke<MetadataRejection[]>("list_metadata_rejections");
      rejections = res ?? [];
    } catch (e: any) {
      console.error("Failed to load metadata rejections:", e);
      toastStore.show(e.toString(), "error");
    } finally {
      isLoading = false;
    }
  }

  async function handleUnblock(id: number) {
    isUnblocking = id;
    try {
      await invoke("unblock_metadata_rejection", { rejectionId: id });
      rejections = rejections.filter((r) => r.id !== id);
      toastStore.show(i18n.t("settings.unblockSuccess", {}, "Metadata match unblocked"), "success");
    } catch (e: any) {
      console.error("Failed to unblock rejection:", e);
      toastStore.show(e.toString(), "error");
    } finally {
      isUnblocking = null;
    }
  }

  onMount(loadRejections);
</script>

<Modal {onClose} maxWidth="max-w-2xl">
  <div
    class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main"
  >
    <div class="flex items-center gap-2">
      <Prohibit class="w-4 h-4 text-brand-accent-text" />
      <h3 class="text-sm font-bold">
        {i18n.t("settings.blockedMetadataTitle", {}, "Blocked Online Metadata")}
      </h3>
    </div>
    <button
      onclick={onClose}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
      aria-label={i18n.t("settings.cancel", {}, "Close")}
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-250px)]">
    {#if isLoading}
      <div class="py-12 flex justify-center items-center">
        <LoadingSpinner label={i18n.t("common.loading", {}, "Loading…")} size="sm" />
      </div>
    {:else if rejections.length === 0}
      <div
        class="py-12 text-center text-brand-text-secondary text-xs flex flex-col items-center gap-2"
      >
        <ShieldSlash class="w-8 h-8 opacity-40" />
        <p>{i18n.t("settings.blockedMetadataEmpty", {}, "No blocked metadata matches.")}</p>
      </div>
    {:else}
      <div class="space-y-2">
        <p class="text-xs text-brand-text-secondary pb-2">
          {i18n.t(
            "settings.blockedMetadataSubtitle",
            {},
            "Online matches you previously rejected. Unblocking allows the engine to associate them again.",
          )}
        </p>
        <div
          class="border border-brand-border rounded-xl overflow-hidden divide-y divide-brand-border bg-brand-main"
        >
          {#each rejections as r (r.id)}
            <div class="flex items-center justify-between gap-4 p-3 text-xs">
              <div class="flex flex-col min-w-0">
                <div class="flex items-center gap-2">
                  <span
                    class="font-semibold text-brand-text-primary uppercase tracking-wide text-[11px] px-1.5 py-0.5 rounded bg-brand-sidebar border border-brand-border"
                  >
                    {r.category}
                  </span>
                  <span class="text-brand-text-primary font-medium">{r.provider}</span>
                </div>
                {#if r.entity_label}
                  <div class="text-[12px] font-medium text-brand-text-primary truncate mt-0.5">
                    {r.entity_label}
                  </div>
                  <div class="text-[10px] text-brand-text-secondary truncate">
                    {r.entity_type}: {r.entity_id}
                  </div>
                {:else}
                  <div class="text-[11px] text-brand-text-secondary truncate mt-0.5">
                    {r.entity_type}: {r.entity_id}
                  </div>
                {/if}
              </div>
              <Button
                onclick={() => handleUnblock(r.id)}
                disabled={isUnblocking === r.id}
                variant="secondary"
                size="sm"
              >
                <Trash class="w-3.5 h-3.5" />
                {i18n.t("settings.unblockBtn", {}, "Unblock")}
              </Button>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</Modal>
