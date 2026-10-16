import { describe, expect, it } from 'vitest';
import { execSync } from 'node:child_process';

describe('install-git-hooks', () => {
	it('executes scripts/install-git-hooks.ts without error', () => {
		expect(() => {
			execSync('bun run scripts/install-git-hooks.ts', { stdio: 'pipe' });
		}).not.toThrow();

		const hooksPath = execSync('git config --get core.hooksPath', { encoding: 'utf-8' }).trim();
		expect(hooksPath).toBe('.githooks');
	});
});
