import { isLinux, isWindows } from "./platform";
import type { AudioBackendKind } from "./types";

/** The i18n key naming each output mode, in Settings and the fallback toast. */
export const AUDIO_BACKEND_LABEL_KEYS: Record<AudioBackendKind, string> = {
  cpal: "settings.audioBackendShared",
  wasapi_exclusive: "settings.audioBackendExclusive",
  asio: "settings.audioBackendAsio",
  alsa: "settings.audioBackendAlsa",
};

/** The output modes this platform offers, shared first; empty when there is no choice to make. */
export function offeredAudioBackends(): AudioBackendKind[] {
  if (isWindows) return ["cpal", "wasapi_exclusive", "asio"];
  if (isLinux) return ["cpal", "alsa"];
  return [];
}

/**
 * Whether a mode opens a driver or hardware device directly, with no system
 * default to follow: with nothing chosen it plays on the first one listed.
 */
export function isDirectHardware(backend: AudioBackendKind): boolean {
  return backend === "asio" || backend === "alsa";
}
