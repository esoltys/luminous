#!/usr/bin/env bun
/**
 * Thin wrapper around `tauri` that sets env vars conditionally on the
 * subcommand, since `package.json` scripts can't branch on argv.
 *
 * - `NO_STRIP=true` always (needed for `tauri build` per README; harmless
 *   for other subcommands).
 * - `LUMINOUS_REMOTE_DEVTOOLS=true` only for `tauri dev` — it must not leak
 *   into `tauri build` (including `--debug` builds), which would silently
 *   enable the loopback inspector server and the "Luminous Debug" app name.
 *
 * Usage: bun run scripts/tauri.ts <tauri args...>
 */

const args = process.argv.slice(2);
const isDev = args[0] === "dev";

const env: Record<string, string> = {
  ...process.env,
  NO_STRIP: "true",
};
if (isDev) {
  env.LUMINOUS_REMOTE_DEVTOOLS = "true";
}

const proc = Bun.spawn(["tauri", ...args], {
  env,
  stdio: ["inherit", "inherit", "inherit"],
});

process.exit(await proc.exited);
