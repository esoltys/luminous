<script lang="ts">
  import { StarIcon as Star, LinkIcon as Link } from "phosphor-svelte";
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
    siWikidata,
    siImdb,
    siInternetarchive,
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
    wikidata: siWikidata,
    imdb: siImdb,
    internet_archive: siInternetarchive,
  };

  const brandIcon = $derived(BRAND_ICONS[normalized]);
</script>

{#if normalized === "website"}
  <!-- Official sites don't have their own brand mark, so a Star (rather
       than a generic link/globe) flags them as the artist's/release's own
       page — platforms with a real brand icon (e.g. "internet_archive")
       never reach this branch (#1123). -->
  <Star {size} class={className} />
{:else if normalized === "listenbrainz"}
  <!-- No monochrome vector for ListenBrainz's logo is available locally, so
       the raster icon is recolored to the theme's currentColor via a CSS
       mask instead of rendering its own full-color artwork — every other
       icon here is monochrome and adapts to hover/theme, and a lone
       colored icon stood out against that (#1123). -->
  <div
    class={className}
    role="img"
    aria-label="ListenBrainz"
    style="width:{size}px;height:{size}px;background-color:currentColor;-webkit-mask-image:url('/listenbrainz-icon.svg');mask-image:url('/listenbrainz-icon.svg');-webkit-mask-size:contain;mask-size:contain;-webkit-mask-repeat:no-repeat;mask-repeat:no-repeat;-webkit-mask-position:center;mask-position:center;"
  ></div>
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
