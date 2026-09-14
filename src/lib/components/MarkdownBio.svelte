<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowSquareOut as ExternalLink } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";

  let {
    text,
    clampLines = 4,
    disableClamp = false,
    showMoreLabel,
    showLessLabel,
    class: className = "",
  }: {
    text: string | null | undefined;
    clampLines?: number;
    disableClamp?: boolean;
    showMoreLabel?: string;
    showLessLabel?: string;
    class?: string;
  } = $props();

  let isExpanded = $state(false);
  let needsClamp = $state(false);
  let paragraphEl = $state<HTMLParagraphElement | null>(null);

  interface BioSegment {
    type: "text" | "link";
    content: string;
    url?: string;
  }

  // Parses markdown links [title](url) and autolinks bare URLs (https://...)
  // Supports balanced parentheses in URLs (e.g. Wikipedia disambiguation URLs)
  function parseBioText(raw: string): BioSegment[] {
    if (!raw) return [];

    const segments: BioSegment[] = [];

    // Match [Title](https://...) supporting balanced parens
    const mdRegex = /\[([^\]]+)\]\((https?:\/\/(?:[^\s()]|\((?:[^\s()]|\([^\s()]*\))*\))+)\)/g;

    let lastIndex = 0;
    let mdMatch: RegExpExecArray | null;

    while ((mdMatch = mdRegex.exec(raw)) !== null) {
      const matchStart = mdMatch.index;
      const matchEnd = mdRegex.lastIndex;

      if (matchStart > lastIndex) {
        appendWithBareUrls(raw.slice(lastIndex, matchStart), segments);
      }

      segments.push({
        type: "link",
        content: mdMatch[1],
        url: mdMatch[2],
      });

      lastIndex = matchEnd;
    }

    if (lastIndex < raw.length) {
      appendWithBareUrls(raw.slice(lastIndex), segments);
    }

    return segments;
  }

  function appendWithBareUrls(chunk: string, segments: BioSegment[]) {
    const urlRegex = /(https?:\/\/[^\s\],]+)/g;
    let lastIndex = 0;
    let urlMatch: RegExpExecArray | null;

    while ((urlMatch = urlRegex.exec(chunk)) !== null) {
      const matchStart = urlMatch.index;
      if (matchStart > lastIndex) {
        segments.push({
          type: "text",
          content: chunk.slice(lastIndex, matchStart),
        });
      }

      let url = urlMatch[1];
      // Trim trailing punctuation preserving balanced parens
      while (url.length > 0) {
        const lastChar = url[url.length - 1];
        if (/[.,;:]/.test(lastChar)) {
          url = url.slice(0, -1);
        } else if (lastChar === ")") {
          const openCount = (url.match(/\(/g) || []).length;
          const closeCount = (url.match(/\)/g) || []).length;
          if (closeCount > openCount) {
            url = url.slice(0, -1);
          } else {
            break;
          }
        } else {
          break;
        }
      }

      segments.push({
        type: "link",
        content: url,
        url,
      });

      lastIndex = matchStart + urlMatch[1].length;
    }

    if (lastIndex < chunk.length) {
      segments.push({
        type: "text",
        content: chunk.slice(lastIndex),
      });
    }
  }

  const parsedSegments = $derived(parseBioText(text ?? ""));

  async function handleOpenUrl(url: string) {
    if (!url) return;
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank");
    }
  }

  function checkClamp() {
    if (!paragraphEl || disableClamp) {
      needsClamp = false;
      return;
    }
    needsClamp = paragraphEl.scrollHeight > paragraphEl.clientHeight + 4;
  }

  $effect(() => {
    // Re-check clamping when text, clampLines or disableClamp changes
    const _ = [text, clampLines, disableClamp];
    requestAnimationFrame(checkClamp);
  });

  onMount(() => {
    checkClamp();
    const observer = new ResizeObserver(checkClamp);
    if (paragraphEl) observer.observe(paragraphEl);
    return () => observer.disconnect();
  });
</script>

{#if text}
  <div class="text-xs text-brand-text-secondary leading-relaxed {className}">
    <p
      bind:this={paragraphEl}
      style={!disableClamp && needsClamp && !isExpanded
        ? `display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: ${clampLines}; overflow: hidden;`
        : undefined}
      class="whitespace-pre-line break-words"
    >
      {#each parsedSegments as segment}
        {#if segment.type === "link" && segment.url}
          <button
            type="button"
            onclick={() => handleOpenUrl(segment.url!)}
            class="inline-flex items-center gap-0.5 text-brand-accent hover:underline font-medium cursor-pointer align-baseline"
            title={segment.url}
          >
            <span>{segment.content}</span>
            <ExternalLink class="w-3 h-3 inline-block opacity-70 shrink-0" />
          </button>
        {:else}
          <span>{segment.content}</span>
        {/if}
      {/each}
    </p>

    {#if !disableClamp && needsClamp}
      <button
        type="button"
        onclick={() => { isExpanded = !isExpanded; }}
        class="mt-1 text-xs font-semibold text-brand-accent hover:underline inline-flex items-center gap-0.5 cursor-pointer select-none"
      >
        {isExpanded
          ? (showLessLabel ?? i18n.t("artistDetail.showLess", {}, "Show less"))
          : (showMoreLabel ?? i18n.t("artistDetail.showMore", {}, "Show more"))}
      </button>
    {/if}
  </div>
{/if}