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
 * The modes that open a driver or hardware device directly, with no system
 * default to follow (with nothing chosen they play on the first device
 * listed), and the i18n key saying none was found.
 */
const DIRECT_HARDWARE_NO_DEVICES_KEYS: Partial<Record<AudioBackendKind, string>> = {
  asio: "settings.audioAsioNoDevices",
  alsa: "settings.audioAlsaNoDevices",
};

export function isDirectHardware(backend: AudioBackendKind): boolean {
  return backend in DIRECT_HARDWARE_NO_DEVICES_KEYS;
}

/** The message shown when a direct-hardware mode finds no devices. */
export function noDevicesKey(backend: AudioBackendKind): string | undefined {
  return DIRECT_HARDWARE_NO_DEVICES_KEYS[backend];
}
