<script lang="ts">
  import { i18n } from "../../stores/i18n.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import ConfirmDialog from "../ConfirmDialog.svelte";

  // Offers to import the station-list playlist files the user just opened or
  // dropped (#2279). Shown while playerStore holds any; either answer clears them.
  const lists = $derived(playerStore.pendingStationLists);
  const stationCount = $derived(lists.reduce((sum, l) => sum + l.station_count, 0));
  const fileNames = $derived(lists.map((l) => l.path.split(/[\\/]/).pop()).join(", "));
</script>

{#if lists.length > 0}
  <ConfirmDialog
    title={i18n.t("radio.importStationListTitle")}
    message={i18n.plural("radio.importStationListMessage", stationCount, { files: fileNames })}
    confirmLabel={i18n.t("radio.importStationListConfirm")}
    cancelLabel={i18n.t("common.cancel")}
    danger={false}
    onConfirm={() => void radioStore.importStationLists(playerStore.takePendingStationLists())}
    onCancel={() => playerStore.takePendingStationLists()}
  />
{/if}
