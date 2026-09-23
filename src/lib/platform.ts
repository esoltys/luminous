// Single source of truth for OS sniffing. Computed once at module load
// instead of re-reading navigator.userAgent/navigator.platform at each
// call site (see #440 — five call sites had drifted into three slightly
// different implementations).

const userAgent = typeof navigator !== "undefined" ? navigator.userAgent : "";
const platform = typeof navigator !== "undefined" ? navigator.platform : "";

export const isWindows = userAgent.includes("Windows") || platform.includes("Win");
