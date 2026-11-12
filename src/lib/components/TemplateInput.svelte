<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import HelpTip from "./HelpTip.svelte";

  /**
   * A folder-and-file-name template field in the Organize syntax
   * (`%albumartist/{%year - }%title`), with buttons that append each
   * placeholder. Shared by Organize and a device's "Folders on the device".
   */
  let {
    value = $bindable(),
    id,
    onchange,
  }: {
    value: string;
    id: string;
    /** After a placeholder is appended or the field is edited. */
    onchange?: (value: string) => void;
  } = $props();

  const VARIABLE_CHIPS = [
    { label: "%albumartist", descKey: "organizer.chipAlbumArtist" },
    { label: "%artist", descKey: "organizer.chipArtist" },
    { label: "%album", descKey: "organizer.chipAlbum" },
    { label: "{%album/}", descKey: "organizer.chipOptionalAlbumFolder" },
    { label: "/", descKey: "organizer.chipFolderSeparator" },
    { label: "{%disc-}", descKey: "organizer.chipConditionalDisc" },
    { label: "{ - %disctitle}", descKey: "organizer.chipConditionalDiscSubtitle" },
    { label: "%track", descKey: "organizer.chipTrack" },
    { label: "{%track }", descKey: "organizer.chipOptionalTrack" },
    { label: "{%track. }", descKey: "organizer.chipOptionalTrackDot" },
    { label: "%title", descKey: "organizer.chipTitle" },
    { label: "%year", descKey: "organizer.chipYear" },
    { label: "%genre", descKey: "organizer.chipGenre" },
  ];

  function insertChip(chipText: string) {
    value = value + chipText;
    onchange?.(value);
  }
</script>

<!-- Plain, directly-editable input, bordered so it reads as editable -->
<input
  {id}
  type="text"
  bind:value
  oninput={() => onchange?.(value)}
  class="w-full px-3.5 py-2.5 rounded-xl bg-brand-sidebar/80 border border-brand-border/80 focus:outline-none focus:border-brand-accent text-brand-text-primary caret-brand-accent font-mono text-xs leading-normal tracking-normal transition-colors min-h-[40px]"
  spellcheck="false"
/>

<div class="flex flex-wrap items-center gap-1.5 pt-1">
  <span class="text-[11px] text-brand-text-secondary mr-1">{i18n.t("organizer.placeholders")}:</span
  >
  {#each VARIABLE_CHIPS as chip}
    {@const shown = chip.label === "/" ? i18n.t("organizer.chipFolderLabel") : chip.label}
    <HelpTip
      text={i18n.t(chip.descKey)}
      label={shown}
      onclick={() => insertChip(chip.label)}
      class="px-2 py-0.5 rounded-lg text-[11px] font-mono transition-colors border bg-brand-sidebar hover:bg-brand-accent/15 border-brand-border/80 text-brand-text-primary hover:text-brand-accent-text font-medium"
    >
      {shown}
    </HelpTip>
  {/each}
</div>
