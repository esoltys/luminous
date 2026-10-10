import type { Component } from "svelte";
import {
  CalendarIcon,
  ClockIcon,
  GaugeIcon,
  HeartIcon,
  HourglassIcon,
  SparkleIcon,
  StackIcon,
  SunHorizonIcon,
  TagIcon,
  TrendUpIcon,
  WarningIcon,
} from "phosphor-svelte";

export type PlaylistCardThemeKind =
  | "queue"
  | "genre"
  | "decade"
  | "smart"
  | "bpm"
  | "artist_tag"
  | "favourites"
  | "recently_added"
  | "most_played"
  | "history"
  | "missing_metadata"
  | "missing_musicbrainz"
  | "daypart";

/**
 * One entry per playlist kind: the single owner of that kind's icon and colour. Every surface that
 * shows a playlist kind (cards, row cards, detail view, sidebar pins) reads from here so they cannot
 * drift apart.
 */
export interface PlaylistCardTheme {
  /** Phosphor glyph that identifies the kind. */
  icon: Component<any>;
  /** Flat tinted frame (background + border) for the small icon tile on row cards. */
  tintClass: string;
  /** Gradient/border/shadow classes for the cover-art frame (Tailwind `bg-gradient-to-br {gradientClass}`). */
  gradientClass: string;
  iconColorClass: string;
  badgeColorClass: string;
}

const THEMES: Record<PlaylistCardThemeKind, PlaylistCardTheme> = {
  queue: {
    icon: StackIcon,
    tintClass: "bg-[#3B82F6]/15 border-[#3B82F6]/30",
    gradientClass: "from-[#1D4ED8]/25 to-[#3B82F6]/15 border-[#3B82F6]/30 shadow-[0_0_20px_2px_rgba(59,130,246,0.35)]",
    iconColorClass: "text-[#3B82F6]",
    badgeColorClass: "bg-[#1D4ED8] text-white",
  },
  decade: {
    icon: CalendarIcon,
    tintClass: "bg-[#22D3EE]/15 border-[#22D3EE]/30",
    gradientClass: "from-[#0891B2]/25 to-[#22D3EE]/15 border-[#22D3EE]/30 shadow-[0_0_20px_2px_rgba(34,211,238,0.35)]",
    iconColorClass: "text-[#22D3EE]",
    badgeColorClass: "bg-[#0891B2] text-white",
  },
  genre: {
    icon: TagIcon,
    tintClass: "bg-[#4ADE80]/15 border-[#4ADE80]/30",
    gradientClass: "from-[#16A34A]/25 to-[#4ADE80]/15 border-[#4ADE80]/30 shadow-[0_0_20px_2px_rgba(74,222,128,0.35)]",
    iconColorClass: "text-[#4ADE80]",
    badgeColorClass: "bg-[#16A34A] text-white",
  },
  smart: {
    icon: SparkleIcon,
    tintClass: "bg-[#A855F7]/15 border-[#A855F7]/30",
    gradientClass: "from-[#7E22CE]/25 to-[#A855F7]/15 border-[#A855F7]/30 shadow-[0_0_20px_2px_rgba(168,85,247,0.35)]",
    iconColorClass: "text-[#A855F7]",
    badgeColorClass: "bg-[#7E22CE] text-white",
  },
  bpm: {
    icon: GaugeIcon,
    tintClass: "bg-[#E879F9]/15 border-[#E879F9]/30",
    gradientClass: "from-[#C026D3]/25 to-[#E879F9]/15 border-[#E879F9]/30 shadow-[0_0_20px_2px_rgba(232,121,249,0.35)]",
    iconColorClass: "text-[#E879F9]",
    badgeColorClass: "bg-[#C026D3] text-white",
  },
  artist_tag: {
    icon: TagIcon,
    tintClass: "bg-[#A3E635]/15 border-[#A3E635]/30",
    gradientClass: "from-[#65A30D]/25 to-[#A3E635]/15 border-[#A3E635]/30 shadow-[0_0_20px_2px_rgba(163,230,53,0.35)]",
    iconColorClass: "text-[#A3E635]",
    badgeColorClass: "bg-[#65A30D] text-white",
  },
  favourites: {
    icon: HeartIcon,
    tintClass: "bg-[#F43F5E]/15 border-[#F43F5E]/30",
    gradientClass: "from-[#DB2777]/25 to-[#F43F5E]/15 border-[#F43F5E]/30 shadow-[0_0_20px_2px_rgba(244,63,94,0.35)]",
    iconColorClass: "text-[#F43F5E]",
    badgeColorClass: "bg-[#DB2777] text-white",
  },
  recently_added: {
    icon: ClockIcon,
    tintClass: "bg-[#EAB308]/15 border-[#EAB308]/30",
    gradientClass: "from-[#CA8A04]/25 to-[#EAB308]/15 border-[#EAB308]/30 shadow-[0_0_20px_2px_rgba(234,179,8,0.35)]",
    iconColorClass: "text-[#CA8A04]",
    badgeColorClass: "bg-[#CA8A04] text-white",
  },
  most_played: {
    icon: TrendUpIcon,
    tintClass: "bg-[#F97316]/15 border-[#F97316]/30",
    gradientClass: "from-[#EA580C]/25 to-[#F97316]/15 border-[#F97316]/30 shadow-[0_0_20px_2px_rgba(249,115,22,0.35)]",
    iconColorClass: "text-[#F97316]",
    badgeColorClass: "bg-[#EA580C] text-white",
  },
  history: {
    icon: HourglassIcon,
    tintClass: "bg-[#93C5FD]/15 border-[#93C5FD]/30",
    gradientClass: "from-[#3B82F6]/25 to-[#93C5FD]/15 border-[#93C5FD]/30 shadow-[0_0_20px_2px_rgba(147,197,253,0.35)]",
    iconColorClass: "text-[#60A5FA]",
    badgeColorClass: "bg-[#3B82F6] text-white",
  },
  missing_metadata: {
    icon: WarningIcon,
    tintClass: "bg-[#94A3B8]/15 border-[#94A3B8]/30",
    gradientClass: "from-[#475569]/25 to-[#94A3B8]/15 border-[#94A3B8]/30 shadow-[0_0_20px_2px_rgba(148,163,184,0.35)]",
    iconColorClass: "text-[#94A3B8]",
    badgeColorClass: "bg-[#475569] text-white",
  },
  missing_musicbrainz: {
    icon: SparkleIcon,
    tintClass: "bg-[#CBD5E1]/15 border-[#CBD5E1]/30",
    gradientClass: "from-[#64748B]/25 to-[#CBD5E1]/15 border-[#CBD5E1]/30 shadow-[0_0_20px_2px_rgba(203,213,225,0.35)]",
    iconColorClass: "text-[#CBD5E1]",
    badgeColorClass: "bg-[#64748B] text-white",
  },
  daypart: {
    icon: SunHorizonIcon,
    tintClass: "bg-[#4F5BD5]/15 border-[#4F5BD5]/30",
    gradientClass: "from-[#3A45B0]/25 to-[#626FE8]/15 border-[#626FE8]/30 shadow-[0_0_20px_2px_rgba(98,111,232,0.35)]",
    iconColorClass: "text-[#4F5BD5]",
    badgeColorClass: "bg-[#4F5BD5] text-white",
  },
};

export function getPlaylistCardTheme(kind: PlaylistCardThemeKind): PlaylistCardTheme {
  return THEMES[kind];
}
