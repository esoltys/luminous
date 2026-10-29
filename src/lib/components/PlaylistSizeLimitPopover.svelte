<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { fade, scale } from "../utils/motion";
  import { invoke } from "@tauri-apps/api/core";
  import { portal } from "../utils/portal";
  import { deviceName } from "../utils/devices";
  import type { ConnectedDevice } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { formatStorageSize, STORAGE_GB, STORAGE_MB } from "../utils/formatters";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import Select from "./Select.svelte";
  import { XIcon as X } from "phosphor-svelte";

  interface Props {
    isOpen: boolean;
    anchorEl: HTMLElement | null;
    /** The playlist's current limit in bytes, or `null` for none. */
    limitBytes: number | null;
    /** The device the limit is measured for, or `null` for library files as they are. */
    deviceId: string | null;
    /** Its name as last measured, for when it isn't connected. */
    deviceLabel: string | null;
    /** Whether Refresh can reorder this playlist (a dynamic one with no sort set), which moves the line. */
    refreshReorders: boolean;
    /** Called with the new limit in bytes (`null` removes it) and the device it's measured for. */
    onApply: (bytes: number | null, deviceId: string | null) => void;
    onClose: () => void;
  }

  let { isOpen, anchorEl, limitBytes, deviceId, deviceLabel, refreshReorders, onApply, onClose }: Props = $props();

  /** Common USB stick / SD card capacities. */
  const PRESETS = [2 * STORAGE_GB, 4 * STORAGE_GB, 8 * STORAGE_GB, 16 * STORAGE_GB, 32 * STORAGE_GB, 64 * STORAGE_GB];
  const WIDTH_PX = 320;

  let popoverEl = $state<HTMLDivElement | null>(null);
  let coords = $state({ left: 16, top: 16 });
  // A number input binds a number, or "" while empty.
  let customValue = $state<number | string>("");
  let customUnit = $state<"MB" | "GB">("GB");
  let chosenDevice = $state<string | null>(null);
  let connected = $state<ConnectedDevice[]>([]);

  /** Connected devices, plus the chosen one when it's unplugged. */
  let deviceOptions = $derived.by(() => {
    const options = connected.map((d) => ({ id: d.id, name: deviceName(d) }));
    if (deviceId && !options.some((o) => o.id === deviceId)) {
      options.push({ id: deviceId, name: deviceLabel ?? deviceId });
    }
    return options;
  });

  let customBytes = $derived.by(() => {
    const n = customValue === "" ? NaN : Number(customValue);
    return Number.isFinite(n) && n > 0 ? Math.round(n * (customUnit === "GB" ? STORAGE_GB : STORAGE_MB)) : null;
  });

  function updatePosition() {
    if (!anchorEl || typeof window === "undefined") return;
    const rect = anchorEl.getBoundingClientRect();
    const left = Math.max(16, Math.min(rect.left, window.innerWidth - WIDTH_PX - 16));
    coords = { left, top: rect.bottom + 8 };
  }

  // Pre-effect: position before the popover's first paint, not one frame after it.
  $effect.pre(() => {
    if (isOpen) {
      updatePosition();
      customValue = "";
      chosenDevice = deviceId;
      invoke<ConnectedDevice[]>("get_connected_devices")
        .then((devices) => (connected = devices))
        .catch((e) => console.error("Failed to list connected devices:", e));
    }
  });

  function apply(bytes: number | null) {
    onApply(bytes, chosenDevice);
    onClose();
  }

  /** With a limit already set, picking a device re-measures it straight away. */
  function chooseDevice(id: string | null) {
    chosenDevice = id;
    if (limitBytes !== null) onApply(limitBytes, id);
  }

  function applyCustom() {
    if (customBytes !== null) apply(customBytes);
  }

  function handleWindowClick(e: MouseEvent) {
    if (!isOpen) return;
    const target = e.target as Node | null;
    if (popoverEl && !popoverEl.contains(target) && anchorEl && !anchorEl.contains(target)) {
      onClose();
    }
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && isOpen) onClose();
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} onresize={updatePosition} />

{#if isOpen}
  <div
    use:portal
    bind:this={popoverEl}
    role="dialog"
    aria-label={i18n.t("playlists.sizeLimitTitle")}
    style="left: {coords.left}px; top: {coords.top}px; width: {WIDTH_PX}px;"
    class="fixed z-50 bg-brand-sidebar border border-brand-border rounded-2xl shadow-2xl p-4 text-brand-text-primary space-y-3 select-none"
    in:scale={{ start: 0.95, duration: 150, easing: cubicOut }}
    out:fade={{ duration: 120, easing: cubicOut }}
  >
    <div class="flex items-start justify-between gap-3">
      <h2 class="text-sm font-semibold">{i18n.t("playlists.sizeLimitTitle")}</h2>
      <button
        onclick={onClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-0.5 rounded hover:bg-white/5 transition-colors cursor-pointer"
        aria-label={i18n.t("common.close")}
      >
        <X class="w-4 h-4" />
      </button>
    </div>
    <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t("playlists.sizeLimitHint")}</p>

    <label class="flex flex-col gap-1.5 text-xs font-medium text-brand-text-primary">
      {i18n.t("playlists.sizeLimitDeviceLabel")}
      <Select
        value={chosenDevice ?? ""}
        onchange={(e) => chooseDevice(e.currentTarget.value || null)}
        class="rounded-lg border border-brand-border bg-brand-main text-brand-text-primary text-xs py-1.5 pl-2.5 pr-8"
      >
        <option value="">{i18n.t("playlists.sizeLimitDeviceNone")}</option>
        {#each deviceOptions as option (option.id)}
          <option value={option.id}>{option.name}</option>
        {/each}
      </Select>
    </label>
    {#if chosenDevice}
      <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t("playlists.sizeLimitDeviceHint")}</p>
    {/if}

    <div class="grid grid-cols-3 gap-2">
      {#each PRESETS as bytes (bytes)}
        <button
          onclick={() => apply(bytes)}
          aria-pressed={limitBytes === bytes}
          class="rounded-full border px-2 py-1.5 text-xs font-semibold transition-colors cursor-pointer
            {limitBytes === bytes
              ? 'border-brand-accent bg-brand-accent/15 text-brand-accent-text'
              : 'border-brand-border/60 hover:bg-brand-main text-brand-text-primary'}"
        >
          {formatStorageSize(bytes)}
        </button>
      {/each}
    </div>

    <div class="flex items-center gap-2">
      <Input
        bind:value={customValue}
        type="number"
        size="sm"
        class="min-w-0 flex-1"
        placeholder={i18n.t("playlists.sizeLimitCustomPlaceholder")}
        onkeydown={(e) => {
          if (e.key === "Enter") applyCustom();
        }}
      />
      <Select
        value={customUnit}
        onchange={(e) => (customUnit = e.currentTarget.value as "MB" | "GB")}
        class="rounded-lg border border-brand-border bg-brand-main text-brand-text-primary text-xs py-1.5 pl-2.5 pr-8"
      >
        <option value="MB">MB</option>
        <option value="GB">GB</option>
      </Select>
      <Button size="sm" variant="secondary" disabled={customBytes === null} onclick={applyCustom}>
        {i18n.t("playlists.sizeLimitSetCustom")}
      </Button>
    </div>

    {#if refreshReorders}
      <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t("playlists.sizeLimitSmartNote")}</p>
    {/if}

    {#if limitBytes !== null}
      <div class="pt-3 border-t border-brand-border/60">
        <Button size="sm" variant="secondary" onclick={() => apply(null)}>
          {i18n.t("playlists.sizeLimitRemove")}
        </Button>
      </div>
    {/if}
  </div>
{/if}
