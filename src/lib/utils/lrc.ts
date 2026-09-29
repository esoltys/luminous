interface LyricWord {
  timeMs: number;
  text: string;
}

interface LyricLine {
  timeMs: number;
  text: string;
  words?: LyricWord[];
}

export interface ParsedLrc {
  lines: LyricLine[];
  fileOffsetMs: number;
}

const LINE_TIME_REGEX = /\[(\d+):(\d+)(?:[.:](\d+))?\]/g;
const WORD_TIME_REGEX = /<(\d+):(\d+)(?:[.:](\d+))?>/g;
const OFFSET_REGEX = /\[offset:\s*([+-]?\d+)\s*\]/i;
const CJK_THAI_REGEX = /[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\u0e00-\u0e7f]/;

function parseTimestamp(minStr: string, secStr: string, fracStr?: string): number {
  const minutes = parseInt(minStr, 10);
  const seconds = parseInt(secStr, 10);
  let ms = 0;
  if (fracStr) {
    if (fracStr.length === 1) {
      ms = parseInt(fracStr, 10) * 100;
    } else if (fracStr.length === 2) {
      ms = parseInt(fracStr, 10) * 10;
    } else {
      ms = parseInt(fracStr.slice(0, 3), 10);
    }
  }
  return minutes * 60 * 1000 + seconds * 1000 + ms;
}

/**
 * Split an unbroken CJK or Thai word cue into natural sub-word segments using Intl.Segmenter.
 */
function segmentWordCue(
  text: string,
  cueStartRawMs: number,
  cueNextRawMs: number | undefined,
  totalOffset: number
): LyricWord[] {
  if (typeof Intl !== "undefined" && Intl.Segmenter && CJK_THAI_REGEX.test(text) && !/\s/.test(text.trim()) && text.trim().length > 1) {
    try {
      const segmenter = new Intl.Segmenter(undefined, { granularity: "word" });
      const segments = Array.from(segmenter.segment(text));
      if (segments.length > 1) {
        const duration = cueNextRawMs !== undefined && cueNextRawMs > cueStartRawMs
          ? cueNextRawMs - cueStartRawMs
          : 0;

        return segments.map((seg) => {
          const frac = seg.index / text.length;
          const segRawMs = cueStartRawMs + Math.round(duration * frac);
          return {
            timeMs: Math.max(0, segRawMs - totalOffset),
            text: seg.segment,
          };
        });
      }
    } catch {
      // Fallback if segmenter throws
    }
  }

  return [
    {
      timeMs: Math.max(0, cueStartRawMs - totalOffset),
      text,
    },
  ];
}

/**
 * Parse raw LRC lyrics string into structured lyric lines and words,
 * honoring file [offset:±ms] header and optional user timing offset.
 *
 * A positive offset value makes lyrics appear earlier (timeMs decreased),
 * per the standard LRC specification.
 */
export function parseLrc(lyricsText: string, userOffsetMs: number = 0): ParsedLrc {
  if (!lyricsText) {
    return { lines: [], fileOffsetMs: 0 };
  }

  let cleanText = lyricsText;
  if (cleanText.startsWith("[synced:false]\n")) {
    cleanText = cleanText.substring("[synced:false]\n".length);
  } else if (cleanText.startsWith("[synced:false]")) {
    cleanText = cleanText.substring("[synced:false]".length);
  }

  // Parse [offset:±ms] header if present
  let fileOffsetMs = 0;
  const offsetMatch = cleanText.match(OFFSET_REGEX);
  if (offsetMatch) {
    fileOffsetMs = parseInt(offsetMatch[1], 10) || 0;
  }

  const totalOffset = fileOffsetMs + userOffsetMs;
  const rawLines = cleanText.split("\n");
  const parsedLines: LyricLine[] = [];

  for (const line of rawLines) {
    const lineMatches: { timeMs: number }[] = [];
    let match: RegExpExecArray | null;

    LINE_TIME_REGEX.lastIndex = 0;
    while ((match = LINE_TIME_REGEX.exec(line)) !== null) {
      const rawMs = parseTimestamp(match[1], match[2], match[3]);
      lineMatches.push({ timeMs: rawMs });
    }

    if (lineMatches.length === 0) {
      continue;
    }

    // Strip line timestamps
    const lineContent = line.replace(LINE_TIME_REGEX, "").trim();

    // Check for word tags `<mm:ss.xx>` inside the line
    const wordTags: { index: number; rawMs: number; tagLength: number }[] = [];
    WORD_TIME_REGEX.lastIndex = 0;
    while ((match = WORD_TIME_REGEX.exec(lineContent)) !== null) {
      const rawMs = parseTimestamp(match[1], match[2], match[3]);
      wordTags.push({
        index: match.index,
        rawMs,
        tagLength: match[0].length,
      });
    }

    const cleanLineText = lineContent.replace(WORD_TIME_REGEX, "").trim();

    let baseWords: LyricWord[] | undefined;
    if (wordTags.length > 0) {
      const extractedWords: LyricWord[] = [];
      const firstLineRawMs = lineMatches[0].timeMs;

      // Handle any text preceding the first word tag
      if (wordTags[0].index > 0) {
        const prefix = lineContent.slice(0, wordTags[0].index);
        if (prefix.trim().length > 0) {
          extractedWords.push(
            ...segmentWordCue(prefix, firstLineRawMs, wordTags[0].rawMs, totalOffset)
          );
        }
      }

      // Handle each word tag and subsequent text
      for (let i = 0; i < wordTags.length; i++) {
        const currentTag = wordTags[i];
        const nextTag = i + 1 < wordTags.length ? wordTags[i + 1] : undefined;
        const start = currentTag.index + currentTag.tagLength;
        const end = nextTag ? nextTag.index : lineContent.length;
        const text = lineContent.slice(start, end);

        if (text.length > 0) {
          extractedWords.push(
            ...segmentWordCue(text, currentTag.rawMs, nextTag?.rawMs, totalOffset)
          );
        }
      }

      if (extractedWords.length > 0) {
        baseWords = extractedWords;
      }
    }

    // For each line timestamp tag on this line
    for (let li = 0; li < lineMatches.length; li++) {
      const lineRawMs = lineMatches[li].timeMs;
      const lineEffectiveMs = Math.max(0, lineRawMs - totalOffset);

      if (baseWords && baseWords.length > 0) {
        const delta = lineRawMs - lineMatches[0].timeMs;
        const adjustedWords = delta === 0
          ? baseWords
          : baseWords.map((w) => ({
              timeMs: Math.max(0, w.timeMs + delta),
              text: w.text,
            }));

        parsedLines.push({
          timeMs: lineEffectiveMs,
          text: cleanLineText,
          words: adjustedWords,
        });
      } else {
        parsedLines.push({
          timeMs: lineEffectiveMs,
          text: cleanLineText,
        });
      }
    }
  }

  const sorted = parsedLines.sort((a, b) => a.timeMs - b.timeMs);
  return {
    lines: sorted,
    fileOffsetMs,
  };
}
