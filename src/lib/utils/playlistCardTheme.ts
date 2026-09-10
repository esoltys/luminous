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

export interface PlaylistCardTheme {
  /** Gradient/border/shadow classes for the cover-art frame (Tailwind `bg-gradient-to-br {gradientClass}`). */
  gradientClass: string;
  iconColorClass: string;
  badgeColorClass: string;
}

const THEMES: Record<PlaylistCardThemeKind, PlaylistCardTheme> = {
  queue: {
    gradientClass: "from-[#4338CA]/25 to-[#7C3AED]/15 border-[#7C3AED]/30 shadow-[0_0_20px_2px_rgba(124,58,237,0.35)]",
    iconColorClass: "text-[#7C3AED]",
    badgeColorClass: "bg-[#7C3AED] text-white",
  },
  decade: {
    gradientClass: "from-[#2563EB]/25 to-[#38BDF8]/15 border-[#38BDF8]/30 shadow-[0_0_20px_2px_rgba(56,189,248,0.35)]",
    iconColorClass: "text-[#38BDF8]",
    badgeColorClass: "bg-[#2563EB] text-white",
  },
  genre: {
    gradientClass: "from-[#059669]/25 to-[#34D399]/15 border-[#34D399]/30 shadow-[0_0_20px_2px_rgba(52,211,153,0.35)]",
    iconColorClass: "text-[#34D399]",
    badgeColorClass: "bg-[#059669] text-white",
  },
  smart: {
    gradientClass: "from-[#C2410C]/25 to-[#F59E0B]/15 border-[#F59E0B]/30 shadow-[0_0_20px_2px_rgba(245,158,11,0.35)]",
    iconColorClass: "text-[#F59E0B]",
    badgeColorClass: "bg-[#C2410C] text-white",
  },
  bpm: {
    gradientClass: "from-[#C026D3]/25 to-[#E879F9]/15 border-[#E879F9]/30 shadow-[0_0_20px_2px_rgba(232,121,249,0.35)]",
    iconColorClass: "text-[#E879F9]",
    badgeColorClass: "bg-[#C026D3] text-white",
  },
  artist_tag: {
    gradientClass: "from-[#EA580C]/25 to-[#FB923C]/15 border-[#FB923C]/30 shadow-[0_0_20px_2px_rgba(251,146,60,0.35)]",
    iconColorClass: "text-[#FB923C]",
    badgeColorClass: "bg-[#EA580C] text-white",
  },
  favourites: {
    gradientClass: "from-[#DB2777]/25 to-[#F43F5E]/15 border-[#F43F5E]/30 shadow-[0_0_20px_2px_rgba(244,63,94,0.35)]",
    iconColorClass: "text-[#F43F5E]",
    badgeColorClass: "bg-[#DB2777] text-white",
  },
  recently_added: {
    gradientClass: "from-[#CA8A04]/25 to-[#FACC15]/15 border-[#FACC15]/30 shadow-[0_0_20px_2px_rgba(250,204,21,0.35)]",
    iconColorClass: "text-[#CA8A04]",
    badgeColorClass: "bg-[#CA8A04] text-white",
  },
  most_played: {
    gradientClass: "from-[#DC2626]/25 to-[#F87171]/15 border-[#F87171]/30 shadow-[0_0_20px_2px_rgba(248,113,113,0.35)]",
    iconColorClass: "text-[#DC2626]",
    badgeColorClass: "bg-[#DC2626] text-white",
  },
  history: {
    gradientClass: "from-[#8B5CF6]/25 to-[#A78BFA]/15 border-[#A78BFA]/30 shadow-[0_0_20px_2px_rgba(167,139,250,0.35)]",
    iconColorClass: "text-[#8B5CF6]",
    badgeColorClass: "bg-[#8B5CF6] text-white",
  },
  missing_metadata: {
    gradientClass: "from-amber-600/25 to-amber-400/15 border-amber-400/30 shadow-[0_0_20px_2px_rgba(245,158,11,0.35)]",
    iconColorClass: "text-amber-500",
    badgeColorClass: "bg-amber-600 text-white",
  },
  missing_musicbrainz: {
    gradientClass: "from-indigo-600/25 to-indigo-400/15 border-indigo-400/30 shadow-[0_0_20px_2px_rgba(99,102,241,0.35)]",
    iconColorClass: "text-indigo-400",
    badgeColorClass: "bg-indigo-600 text-white",
  },
  daypart: {
    gradientClass: "from-[#0D9488]/25 to-[#2DD4BF]/15 border-[#2DD4BF]/30 shadow-[0_0_20px_2px_rgba(45,212,191,0.35)]",
    iconColorClass: "text-[#2DD4BF]",
    badgeColorClass: "bg-[#0D9488] text-white",
  },
};

export function getPlaylistCardTheme(kind: PlaylistCardThemeKind): PlaylistCardTheme {
  return THEMES[kind];
}
