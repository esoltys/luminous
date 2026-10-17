import { describe, expect, it } from 'vitest';
import { execSync } from 'node:child_process';

describe('install-git-hooks', () => {
	it('executes scripts/install-git-hooks.ts without error', () => {
		expect(() => {
			execSync('bun run scripts/install-git-hooks.ts', { stdio: 'pipe' });
		}).not.toThrow();

		// --local: the scope the script writes. A worktree's own config.worktree
		// may set its own hooksPath, which would win an unscoped read.
		const hooksPath = execSync('git config --local --get core.hooksPath', { encoding: 'utf-8' }).trim();
		expect(hooksPath).toBe('.githooks');
	});
});
