<script lang="ts">
  import { Globe, Link } from "lucide-svelte";
  import {
    siThreads,
    siSpotify,
    siApplemusic,
    siBandcamp,
    siSoundcloud,
    siYoutube,
    siInstagram,
    siX,
    siFacebook,
    siBluesky,
    siTiktok,
    siMusicbrainz,
    siDiscogs,
    siWikipedia,
    type SimpleIcon,
  } from "simple-icons";

  let {
    platform,
    size = 16,
    class: className = "",
  }: {
    platform: string;
    size?: number;
    class?: string;
  } = $props();

  const normalized = $derived((platform || "").toLowerCase().trim());

  const BRAND_ICONS: Record<string, SimpleIcon> = {
    threads: siThreads,
    spotify: siSpotify,
    apple_music: siApplemusic,
    apple: siApplemusic,
    bandcamp: siBandcamp,
    soundcloud: siSoundcloud,
    youtube: siYoutube,
    instagram: siInstagram,
    x: siX,
    twitter: siX,
    facebook: siFacebook,
    bluesky: siBluesky,
    tiktok: siTiktok,
    musicbrainz: siMusicbrainz,
    discogs: siDiscogs,
    wikipedia: siWikipedia,
  };

  const brandIcon = $derived(BRAND_ICONS[normalized]);
</script>

{#if normalized === "website"}
  <Globe {size} class={className} />
{:else if brandIcon}
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="currentColor"
    class={className}
    aria-hidden="true"
  >
    <path d={brandIcon.path} />
  </svg>
{:else}
  <Link {size} class={className} />
{/if}
