// @vitest-environment node
//
// Regression tests for CI build/test time optimizations (#1824):
// 1. Path-aware scope classification (`scripts/ci-should-run.sh`) so single-layer
//    PRs skip irrelevant heavy workflows while pushes to `main` and schedules
//    always run everything unconditionally.
// 2. CodeQL `paths-ignore` excluding Cargo's `target/` directory so `Analyze (rust)`
//    doesn't spend 14+ minutes indexing 50 MiB of generated FFI bindings in
//    `target/debug/build/**/out/*.rs`.
// 3. Rust `luminous_lib` building only `rlib` (not unused mobile `staticlib`/`cdylib`
//    targets that triple link/LTO time), `Swatinem/rust-cache` caching workspace
//    crates (`cache-all-crates: true`), `lld` linking, and `line-tables-only` debuginfo.
// 4. Vite `phosphorBarrelOptimizer()` rewriting named `phosphor-svelte` barrel
//    imports so Vitest workers don't evaluate all 3,025 `.svelte` icon modules.
import { describe, it, expect } from "vitest";
import { execFileSync } from "child_process";
import fs from "fs";
import os from "os";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "../../..");
const ciWorkflow = fs.readFileSync(path.join(root, ".github/workflows/ci.yml"), "utf-8");
const codeqlWorkflow = fs.readFileSync(path.join(root, ".github/workflows/codeql.yml"), "utf-8");
const auditWorkflow = fs.readFileSync(path.join(root, ".github/workflows/audit.yml"), "utf-8");
const tauriCargoToml = fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf-8");
const viteConfig = fs.readFileSync(path.join(root, "vite.config.js"), "utf-8");
const scriptPath = path.join(root, "scripts/ci-should-run.sh");

function runScopeCheck(
  scope: "frontend" | "backend" | "lean" | "actions",
  envOverrides: Record<string, string>
): { stdout: string; githubOutput: string } {
  const tmpFile = path.join(
    os.tmpdir(),
    `ci-should-run-test-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  fs.writeFileSync(tmpFile, "");
  try {
    const stdout = execFileSync("bash", [scriptPath, scope], {
      cwd: root,
      encoding: "utf-8",
      env: {
        ...process.env,
        GITHUB_OUTPUT: tmpFile,
        ...envOverrides,
      },
    }).trim();
    const githubOutput = fs.readFileSync(tmpFile, "utf-8").trim();
    return { stdout, githubOutput };
  } finally {
    fs.rmSync(tmpFile, { force: true });
  }
}

describe("scripts/ci-should-run.sh: path-based CI scope filtering (#1824)", () => {
  it("always returns true on non-PR events (push, schedule, workflow_dispatch)", () => {
    for (const event of ["push", "schedule", "workflow_dispatch", ""]) {
      for (const scope of ["frontend", "backend", "lean", "actions"] as const) {
        const res = runScopeCheck(scope, {
          GITHUB_EVENT_NAME: event,
          CI_CHANGED_FILES: "README.md",
        });
        expect(res.stdout, `event=${event} scope=${scope}`).toBe("true");
        expect(res.githubOutput).toBe("run=true");
      }
    }
  });

  it("fails open (true) on pull_request when changed file list is empty", () => {
    const res = runScopeCheck("backend", {
      GITHUB_EVENT_NAME: "pull_request",
      GITHUB_BASE_REF: "",
      CI_CHANGED_FILES: "",
    });
    expect(res.stdout).toBe("true");
    expect(res.githubOutput).toBe("run=true");
  });

  it("runs only frontend scope on a frontend-only PR", () => {
    const env = {
      GITHUB_EVENT_NAME: "pull_request",
      CI_CHANGED_FILES: "src/lib/components/PlayerBar.svelte\nsrc/lib/locales/en.ts",
    };
    expect(runScopeCheck("frontend", env).stdout).toBe("true");
    expect(runScopeCheck("backend", env).stdout).toBe("false");
    expect(runScopeCheck("lean", env).stdout).toBe("false");
    expect(runScopeCheck("actions", env).stdout).toBe("false");
  });

  it("runs only backend scope on a Rust-only PR", () => {
    const env = {
      GITHUB_EVENT_NAME: "pull_request",
      CI_CHANGED_FILES: "src-tauri/src/player.rs\ncrates/luminous-audio/src/lib.rs",
    };
    expect(runScopeCheck("backend", env).stdout).toBe("true");
    expect(runScopeCheck("frontend", env).stdout).toBe("false");
    expect(runScopeCheck("lean", env).stdout).toBe("false");
    expect(runScopeCheck("actions", env).stdout).toBe("false");
  });

  it("runs lean scope when verification/lean files change", () => {
    const env = {
      GITHUB_EVENT_NAME: "pull_request",
      CI_CHANGED_FILES: "verification/lean/Luminous/Player.lean",
    };
    expect(runScopeCheck("lean", env).stdout).toBe("true");
    expect(runScopeCheck("frontend", env).stdout).toBe("false");
    expect(runScopeCheck("backend", env).stdout).toBe("false");
  });

  it("triggers both frontend and backend when Cargo.lock or src-tauri/Cargo.toml changes", () => {
    const env = {
      GITHUB_EVENT_NAME: "pull_request",
      CI_CHANGED_FILES: "Cargo.lock",
    };
    expect(runScopeCheck("frontend", env).stdout).toBe("true");
    expect(runScopeCheck("backend", env).stdout).toBe("true");
  });

  it("skips heavy scopes on a markdown-only docs PR", () => {
    const env = {
      GITHUB_EVENT_NAME: "pull_request",
      CI_CHANGED_FILES: "README.md\ndocs/CI.md",
    };
    expect(runScopeCheck("frontend", env).stdout).toBe("false");
    expect(runScopeCheck("backend", env).stdout).toBe("false");
    expect(runScopeCheck("lean", env).stdout).toBe("false");
    expect(runScopeCheck("actions", env).stdout).toBe("false");
  });
});

describe("GitHub workflows & Cargo/Vite build optimizations (#1824)", () => {
  it("ci.yml uses lld linker, line-tables-only debuginfo, cache-all-crates, and --lib --tests", () => {
    expect(ciWorkflow).toMatch(/CARGO_PROFILE_DEV_DEBUG:\s*line-tables-only/);
    expect(ciWorkflow).toMatch(/CARGO_PROFILE_TEST_DEBUG:\s*line-tables-only/);
    expect(ciWorkflow).toMatch(/RUSTFLAGS:\s*"-C link-arg=-fuse-ld=lld"/);
    expect(ciWorkflow).toMatch(/cache-all-crates:\s*true/);
    expect(ciWorkflow).toMatch(/cargo test --workspace --lib --tests/);
  });

  it("codeql.yml excludes target/ and node_modules/ from CodeQL source indexing and filters by scope", () => {
    expect(codeqlWorkflow).toMatch(/bash scripts\/ci-should-run\.sh \$\{\{\s*matrix\.scope\s*\}\}/);
    expect(codeqlWorkflow).toMatch(/paths-ignore:\s*\n\s*-\s*target\s*\n\s*-\s*node_modules/);
    expect(codeqlWorkflow).toMatch(/cache-all-crates:\s*true/);
  });

  it("audit.yml gates cargo-deny on backend scope and lockfile check on frontend scope", () => {
    expect(auditWorkflow).toMatch(/bash scripts\/ci-should-run\.sh backend/);
    expect(auditWorkflow).toMatch(/bash scripts\/ci-should-run\.sh frontend/);
  });

  it("src-tauri/Cargo.toml builds only rlib for luminous_lib, not unused mobile staticlib/cdylib", () => {
    expect(tauriCargoToml).toMatch(/name\s*=\s*"luminous_lib"\s*\ncrate-type\s*=\s*\["rlib"\]/);
    expect(tauriCargoToml).not.toMatch(/staticlib|cdylib/);
  });

  it("vite.config.js registers phosphorBarrelOptimizer to avoid barrel-importing 3,025 icons in Vitest", () => {
    expect(viteConfig).toContain("function phosphorBarrelOptimizer()");
    expect(viteConfig).toMatch(/plugins:\s*\[\s*phosphorBarrelOptimizer\(\)/);
  });
});
