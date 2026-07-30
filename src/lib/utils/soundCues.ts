import { prefs } from "../stores/prefs.svelte";

let audioCtx: AudioContext | null = null;

function getAudioContext(): AudioContext | null {
  if (typeof window === "undefined") return null;
  if (!audioCtx) {
    const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
    if (AudioContextClass) {
      audioCtx = new AudioContextClass();
    }
  }
  if (audioCtx && audioCtx.state === "suspended") {
    audioCtx.resume().catch(() => {});
  }
  return audioCtx;
}

function playNote(freq: number, startTime: number, dur: number, gainPeak: number) {
  const ctx = getAudioContext();
  if (!ctx) return;
  try {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = "sine";
    osc.frequency.value = freq;
    gain.gain.setValueAtTime(0, startTime);
    gain.gain.linearRampToValueAtTime(gainPeak, startTime + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.001, startTime + dur);
    osc.connect(gain).connect(ctx.destination);
    osc.start(startTime);
    osc.stop(startTime + dur + 0.02);
  } catch (e) {
    console.error("Failed to play sound cue note:", e);
  }
}

export const soundCues = {
  playSuccess() {
    if (!prefs.soundCuesEnabled) return;
    const ctx = getAudioContext();
    if (!ctx) return;
    const t = ctx.currentTime;
    playNote(660, t, 0.12, 0.14);
    playNote(830, t + 0.06, 0.12, 0.14);
  },

  playNew() {
    if (!prefs.soundCuesEnabled) return;
    const ctx = getAudioContext();
    if (!ctx) return;
    const t = ctx.currentTime;
    playNote(740, t, 0.1, 0.12);
  },

  playMilestone() {
    if (!prefs.soundCuesEnabled) return;
    const ctx = getAudioContext();
    if (!ctx) return;
    const t = ctx.currentTime;
    playNote(523, t, 0.16, 0.15);
    playNote(659, t + 0.09, 0.16, 0.15);
    playNote(880, t + 0.18, 0.22, 0.16);
  },

  playWarning() {
    if (!prefs.soundCuesEnabled) return;
    const ctx = getAudioContext();
    if (!ctx) return;
    const t = ctx.currentTime;
    playNote(220, t, 0.15, 0.14);
  },
};
