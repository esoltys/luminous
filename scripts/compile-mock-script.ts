// Strips the `import type` from tauri-ipc-mock.ts and transpiles it to a
// plain, dependency-free script that can be injected into a page (via
// Playwright's addInitScript or a <script> tag) — neither of which resolve
// ES module imports.
import { readFileSync } from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SOURCE_PATH = path.join(__dirname, "tauri-ipc-mock.ts");

export function compileMockScript(): string {
  const source = readFileSync(SOURCE_PATH, "utf8");
  const withoutTypeImports = source.replace(
    /^import\s+type\s*\{[^}]*\}\s*from\s*["'][^"']+["'];?\s*$/m,
    ""
  );
  const { outputText } = ts.transpileModule(withoutTypeImports, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2020,
      module: ts.ModuleKind.None,
    },
  });
  return outputText;
}
