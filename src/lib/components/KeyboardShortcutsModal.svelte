<script lang="ts">
  import { Keyboard, X } from "lucide-svelte";
  import Modal from "./Modal.svelte";
  import { i18n } from "../stores/i18n.svelte";

  let { onClose }: { onClose: () => void } = $props();

  type ShortcutEntry = { keys: string[]; label: string };
  type ShortcutGroup = { title: string; entries: ShortcutEntry[] };

  let groups = $derived<ShortcutGroup[]>([
    {
      title: i18n.t("shortcuts.groupPlayback"),
      entries: [
        { keys: ["Space"], label: i18n.t("shortcuts.playPause") },
        { keys: ["←"], label: i18n.t("shortcuts.previousTrack") },
        { keys: ["→"], label: i18n.t("shortcuts.nextTrack") },
        { keys: ["↑"], label: i18n.t("shortcuts.volumeUp") },
        { keys: ["↓"], label: i18n.t("shortcuts.volumeDown") },
        { keys: ["Page Up"], label: i18n.t("shortcuts.seekBack") },
        { keys: ["Page Down"], label: i18n.t("shortcuts.seekForward") }
      ]
    },
    {
      title: i18n.t("shortcuts.groupNavigation"),
      entries: [
        { keys: ["Ctrl", "L"], label: i18n.t("shortcuts.focusSearch") },
        { keys: ["Esc"], label: i18n.t("shortcuts.closeSearch") },
        { keys: ["Ctrl", "O"], label: i18n.t("shortcuts.openSong") },
        { keys: ["Ctrl", "["], label: i18n.t("shortcuts.goBack") },
        { keys: ["Ctrl", "]"], label: i18n.t("shortcuts.goForward") }
      ]
    },
    {
      title: i18n.t("shortcuts.groupLayout"),
      entries: [
        { keys: ["Ctrl", "1"], label: i18n.t("shortcuts.toggleSidebar") },
        { keys: ["Ctrl", "2"], label: i18n.t("shortcuts.toggleImmersive") },
        { keys: ["Ctrl", "3"], label: i18n.t("shortcuts.toggleRightPanel") },
        { keys: ["Ctrl", "M"], label: i18n.t("shortcuts.toggleMiniplayer") },
        { keys: ["Ctrl", ","], label: i18n.t("shortcuts.openSettings") },
        { keys: ["Ctrl", "/"], label: i18n.t("shortcuts.showShortcuts") }
      ]
    }
  ]);
</script>

<Modal {onClose} closeOnBackdropClick ariaLabelledby="shortcuts-modal-title" class="max-h-[85vh]">
  <div class="flex items-center justify-between px-6 py-4 border-b border-brand-border/60 bg-brand-main/50 shrink-0">
    <div class="flex items-center gap-2.5">
      <div class="p-2 rounded-xl bg-brand-accent/20 text-brand-accent-text">
        <Keyboard class="w-5 h-5" />
      </div>
      <h2 id="shortcuts-modal-title" class="text-base font-bold text-brand-text-primary">{i18n.t("shortcuts.title")}</h2>
    </div>
    <button
      onclick={onClose}
      class="text-brand-text-secondary hover:text-brand-text-primary p-1.5 rounded-lg hover:bg-brand-main/80 transition-colors"
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="flex-1 overflow-y-auto p-6 flex flex-col gap-6">
    {#each groups as group (group.title)}
      <div>
        <h3 class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider mb-2">{group.title}</h3>
        <div class="rounded-xl border border-brand-border/60 divide-y divide-brand-border/40 overflow-hidden">
          {#each group.entries as entry (entry.label)}
            <div class="flex items-center justify-between gap-4 px-4 py-2.5 bg-brand-main/30">
              <div class="flex items-center gap-1.5 shrink-0">
                {#each entry.keys as key (key)}
                  <kbd class="min-w-[1.75rem] px-2 py-1 rounded-lg bg-brand-main border border-brand-border text-xs font-semibold text-brand-text-primary text-center">
                    {key}
                  </kbd>
                {/each}
              </div>
              <span class="text-sm text-brand-text-secondary text-right">{entry.label}</span>
            </div>
          {/each}
        </div>
      </div>
    {/each}

    <p class="text-xs text-brand-text-secondary/60 text-center">{i18n.t("shortcuts.footnote")}</p>
  </div>
</Modal>
