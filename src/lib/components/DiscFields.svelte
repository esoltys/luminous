<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import FormField from "./FormField.svelte";
  import Input from "./Input.svelte";
  import type { MixedField } from "../utils/mixedField.svelte";

  /** The album editors' Disc # and Disc Subtitle inputs, for a selection
      whose tracks may sit on different discs. Renders two grid cells. */
  let {
    disc,
    subtitle,
    disabled = false,
  }: {
    disc: MixedField<number>;
    subtitle: MixedField<string>;
    disabled?: boolean;
  } = $props();
</script>

<FormField label={i18n.t("albumTagEditor.discField")} for="album-tag-disc">
  <Input
    id="album-tag-disc"
    type="number"
    value={disc.value ?? ""}
    {disabled}
    placeholder={disc.showsMixed ? i18n.t("albumTagEditor.mixedValues") : undefined}
    oninput={(e) => {
      const val = parseInt(e.currentTarget.value, 10);
      disc.set(isNaN(val) ? null : val);
    }}
    size="sm"
    class="w-full"
  />
</FormField>

<FormField label={i18n.t("tagEditor.discSubtitleField")} for="album-tag-disc-subtitle" span2>
  <Input
    id="album-tag-disc-subtitle"
    value={subtitle.value ?? ""}
    {disabled}
    placeholder={subtitle.showsMixed
      ? i18n.t("albumTagEditor.mixedValues")
      : i18n.t("tagEditor.discSubtitlePlaceholder")}
    oninput={(e) => subtitle.set(e.currentTarget.value)}
    size="sm"
    class="w-full"
  />
</FormField>
