import { SCRIPT_API_VERSION, type ScriptContract } from "./types";

type Version = [major: number, minor: number, patch: number];

/** Parses "1", "1.2" or "1.2.3"; missing parts are 0. */
function parseVersion(text: string): Version | null {
  const match = /^(\d+)(?:\.(\d+))?(?:\.(\d+))?$/.exec(text.trim());
  if (!match) return null;
  return [Number(match[1]), Number(match[2] ?? 0), Number(match[3] ?? 0)];
}

function compare(a: Version, b: Version): number {
  return a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
}

/**
 * Every callable on the facade as a dotted path ("wait.settled", "dialogs.welcome.show"),
 * sorted. Data fields such as `version` are not methods and are left out.
 */
export function methodPaths(api: object, prefix = ""): string[] {
  const paths: string[] = [];
  for (const [key, value] of Object.entries(api)) {
    const path = prefix + key;
    if (typeof value === "function") paths.push(path);
    else if (value && typeof value === "object") paths.push(...methodPaths(value, `${path}.`));
  }
  return paths.sort();
}

/**
 * The handshake behind `connect()`: resolves the contract when SCRIPT_API_VERSION
 * satisfies the caret range of `requires` and every required capability is offered,
 * and otherwise throws one message naming everything that is wrong.
 */
export function negotiateContract(requires: string, required: readonly string[], offered: string[]): ScriptContract {
  const needed = parseVersion(requires);
  if (!needed) throw new Error(`connect() needs a version like "1", "1.2" or "1.2.3", not "${requires}".`);
  const running = parseVersion(SCRIPT_API_VERSION)!;

  const problems: string[] = [];
  if (running[0] !== needed[0] || compare(running, needed) < 0) {
    const wanted = needed.join(".");
    problems.push(`does not satisfy ^${wanted} (needs ≥${wanted} <${needed[0] + 1}.0.0)`);
  }
  const missing = required.filter((name) => !offered.includes(name));
  if (missing.length > 0) problems.push(`lacks ${missing.join(", ")}`);
  if (problems.length > 0) throw new Error(`Luminous scripting API ${SCRIPT_API_VERSION} ${problems.join(" and ")}.`);

  return { version: SCRIPT_API_VERSION, capabilities: offered };
}
