<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { splinePath } from "../utils/eqScale";
  import Listbox, { type ListboxGroup, type ListboxItem } from "./Listbox.svelte";
  import {
    userPresetKey,
    type EqMode,
    type EqPresetList,
    type SettingRange,
  } from "../types/equalizer";

  interface Props {
    id?: string;
    value: string | null;
    presets: EqPresetList;
    mode: EqMode;
    previews: Map<string, number[]>;
    gainRange?: SettingRange;
    getLabel?: (name: string) => string;
    onselect: (presetKey: string) => void;
    class?: string;
  }

  let {
    id = "eq-preset-picker",
    value,
    presets,
    mode,
    previews,
    gainRange = { min: -12, max: 12 },
    getLabel,
    onselect,
    class: className = "",
  }: Props = $props();

  function defaultLabel(name: string): string {
    const keyMap: Record<string, string> = {
      "Flat": "flatPreset",
      "Pop": "popPreset",
      "Rock": "rockPreset",
      "Classical": "classicalPreset",
      "Jazz": "jazzPreset",
      "Bass Boost": "bassBoostPreset",
      "Vocal Boost": "vocalBoostPreset",
      "Treble Boost": "trebleBoostPreset",
      "Headphones": "headphonesPreset",
    };
    const key = keyMap[name];
    return key ? i18n.t(`equalizer.${key}`) : name;
  }

  const resolveLabel = (name: string) => (getLabel ? getLabel(name) : defaultLabel(name));

  function curvePathForKey(key: string): string {
    const response = previews.get(key);
    if (!response || response.length < 2) return "";
    return splinePath(response, gainRange);
  }

  let groups = $derived.by<ListboxGroup[]>(() => {
    const result: ListboxGroup[] = [
      { id: "builtin", label: i18n.t("equalizer.builtinPresets") },
    ];
    if (mode === "parametric" && presets.user.length > 0) {
      result.push({ id: "user", label: i18n.t("equalizer.userPresets") });
    }
    return result;
  });

  interface PresetItemData {
    curvePath?: string;
  }

  let items = $derived.by<ListboxItem<PresetItemData>[]>(() => {
    const result: ListboxItem<PresetItemData>[] = [];
    if (value === null) {
      result.push({
        id: `${id}-opt-custom`,
        value: "",
        label: i18n.t("equalizer.customPreset"),
        disabled: true,
      });
    }

    for (const name of presets.builtin) {
      result.push({
        id: `${id}-opt-builtin-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
        value: name,
        label: resolveLabel(name),
        group: "builtin",
        data: {
          curvePath: curvePathForKey(name),
        },
      });
    }

    if (mode === "parametric") {
      for (const userPreset of presets.user) {
        const key = userPresetKey(userPreset.id);
        result.push({
          id: `${id}-opt-user-${userPreset.id}`,
          value: key,
          label: userPreset.name,
          group: "user",
          data: {
            curvePath: curvePathForKey(key),
          },
        });
      }
    }

    return result;
  });
</script>

<Listbox
  {id}
  {value}
  {items}
  {groups}
  ariaLabel={i18n.t("equalizer.presetLabel")}
  placeholder={i18n.t("equalizer.customPreset")}
  onselect={(val) => onselect(val)}
  class={className}
>
  {#snippet trigger({ item })}
    <span class="truncate">
      {item ? item.label : i18n.t("equalizer.customPreset")}
    </span>
    {#if item?.data?.curvePath}
      <svg
        viewBox="0 0 100 40"
        class="w-10 h-3.5 shrink-0 text-brand-accent stroke-current fill-none opacity-80"
        aria-hidden="true"
      >
        <line
          x1="0"
          y1="20"
          x2="100"
          y2="20"
          stroke="currentColor"
          stroke-opacity="0.25"
          stroke-dasharray="3 3"
          stroke-width="1"
        />
        <path d={item.data.curvePath} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    {/if}
  {/snippet}

  {#snippet option({ item, isSelected, isFocused })}
    <span class="truncate">{item.label}</span>
    {#if item.data?.curvePath}
      <svg
        viewBox="0 0 100 40"
        class="w-12 h-4 shrink-0 text-brand-accent stroke-current fill-none {isSelected || isFocused ? 'opacity-100' : 'opacity-70'}"
        aria-hidden="true"
      >
        <line
          x1="0"
          y1="20"
          x2="100"
          y2="20"
          stroke="currentColor"
          stroke-opacity="0.25"
          stroke-dasharray="3 3"
          stroke-width="1"
        />
        <path d={item.data.curvePath} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    {/if}
  {/snippet}
</Listbox>
