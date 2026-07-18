// Derives a short "moodmoji" hash from a track's moodbar RGB strip
// (150 points * 3 bytes, bass=R/mid=G/treble=B — see src-tauri/src/moodbar.rs).
// Two emojis: the first reflects which frequency band dominates the track,
// the second reflects overall spectral energy/intensity.

const BAND_EMOJI: Record<"bass" | "mid" | "treble", string> = {
  bass: "🥁",
  mid: "🎸",
  treble: "🔔",
};

// Intensity ramp, not an emotion axis — a sad, dense track can score
// "intense" here just as easily as a happy one, so these deliberately avoid
// faces (snow=calm/chill, leaf in the wind=balanced, fire=intense).
const ENERGY_EMOJI = ["❄️", "🍃", "🔥"] as const;

export function deriveMoodmoji(moodbarData: number[]): string {
  if (!moodbarData || moodbarData.length < 3) return "🎵";

  const points = Math.floor(moodbarData.length / 3);
  let rSum = 0;
  let gSum = 0;
  let bSum = 0;
  for (let i = 0; i < points; i++) {
    rSum += moodbarData[i * 3];
    gSum += moodbarData[i * 3 + 1];
    bSum += moodbarData[i * 3 + 2];
  }

  const rAvg = rSum / points;
  const gAvg = gSum / points;
  const bAvg = bSum / points;

  const dominant: "bass" | "mid" | "treble" =
    rAvg >= gAvg && rAvg >= bAvg ? "bass" : gAvg >= bAvg ? "mid" : "treble";

  const overallAvg = (rAvg + gAvg + bAvg) / 3;
  const energyIdx = overallAvg < 85 ? 0 : overallAvg < 170 ? 1 : 2;

  return `${BAND_EMOJI[dominant]}${ENERGY_EMOJI[energyIdx]}`;
}
