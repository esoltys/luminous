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
    tintClass: "bg-[#7C3AED]/15 border-[#7C3AED]/30",
    gradientClass: "from-[#4338CA]/25 to-[#7C3AED]/15 border-[#7C3AED]/30 shadow-[0_0_20px_2px_rgba(124,58,237,0.35)]",
    iconColorClass: "text-[#7C3AED]",
    badgeColorClass: "bg-[#7C3AED] text-white",
  },
  decade: {
    icon: CalendarIcon,
    tintClass: "bg-[#38BDF8]/15 border-[#38BDF8]/30",
    gradientClass: "from-[#2563EB]/25 to-[#38BDF8]/15 border-[#38BDF8]/30 shadow-[0_0_20px_2px_rgba(56,189,248,0.35)]",
    iconColorClass: "text-[#38BDF8]",
    badgeColorClass: "bg-[#2563EB] text-white",
  },
  genre: {
    icon: TagIcon,
    tintClass: "bg-[#34D399]/15 border-[#34D399]/30",
    gradientClass: "from-[#059669]/25 to-[#34D399]/15 border-[#34D399]/30 shadow-[0_0_20px_2px_rgba(52,211,153,0.35)]",
    iconColorClass: "text-[#34D399]",
    badgeColorClass: "bg-[#059669] text-white",
  },
  smart: {
    icon: SparkleIcon,
    tintClass: "bg-[#F59E0B]/15 border-[#F59E0B]/30",
    gradientClass: "from-[#C2410C]/25 to-[#F59E0B]/15 border-[#F59E0B]/30 shadow-[0_0_20px_2px_rgba(245,158,11,0.35)]",
    iconColorClass: "text-[#F59E0B]",
    badgeColorClass: "bg-[#C2410C] text-white",
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
    tintClass: "bg-[#FB923C]/15 border-[#FB923C]/30",
    gradientClass: "from-[#EA580C]/25 to-[#FB923C]/15 border-[#FB923C]/30 shadow-[0_0_20px_2px_rgba(251,146,60,0.35)]",
    iconColorClass: "text-[#FB923C]",
    badgeColorClass: "bg-[#EA580C] text-white",
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
    tintClass: "bg-[#FACC15]/15 border-[#FACC15]/30",
    gradientClass: "from-[#CA8A04]/25 to-[#FACC15]/15 border-[#FACC15]/30 shadow-[0_0_20px_2px_rgba(250,204,21,0.35)]",
    iconColorClass: "text-[#CA8A04]",
    badgeColorClass: "bg-[#CA8A04] text-white",
  },
  most_played: {
    icon: TrendUpIcon,
    tintClass: "bg-[#DC2626]/15 border-[#DC2626]/30",
    gradientClass: "from-[#DC2626]/25 to-[#F87171]/15 border-[#F87171]/30 shadow-[0_0_20px_2px_rgba(248,113,113,0.35)]",
    iconColorClass: "text-[#DC2626]",
    badgeColorClass: "bg-[#DC2626] text-white",
  },
  history: {
    icon: HourglassIcon,
    tintClass: "bg-[#8B5CF6]/15 border-[#8B5CF6]/30",
    gradientClass: "from-[#8B5CF6]/25 to-[#A78BFA]/15 border-[#A78BFA]/30 shadow-[0_0_20px_2px_rgba(167,139,250,0.35)]",
    iconColorClass: "text-[#8B5CF6]",
    badgeColorClass: "bg-[#8B5CF6] text-white",
  },
  missing_metadata: {
    icon: WarningIcon,
    tintClass: "bg-amber-500/15 border-amber-500/30",
    gradientClass: "from-amber-600/25 to-amber-400/15 border-amber-400/30 shadow-[0_0_20px_2px_rgba(245,158,11,0.35)]",
    iconColorClass: "text-amber-500",
    badgeColorClass: "bg-amber-600 text-white",
  },
  missing_musicbrainz: {
    icon: SparkleIcon,
    tintClass: "bg-indigo-500/15 border-indigo-500/30",
    gradientClass: "from-indigo-600/25 to-indigo-400/15 border-indigo-400/30 shadow-[0_0_20px_2px_rgba(99,102,241,0.35)]",
    iconColorClass: "text-indigo-400",
    badgeColorClass: "bg-indigo-600 text-white",
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
