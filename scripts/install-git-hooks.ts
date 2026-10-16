import { execSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

try {
	const repoRoot = resolve(import.meta.dirname, '..');
	if (existsSync(resolve(repoRoot, '.git'))) {
		execSync('git config core.hooksPath .githooks', { cwd: repoRoot, stdio: 'ignore' });
	}
} catch {
	// Silently ignore if git is missing or repository configuration fails
}
