import { execSync } from 'node:child_process';
import { existsSync } from 'node:fs';

try {
	if (existsSync('.git')) {
		execSync('git config core.hooksPath .githooks', { stdio: 'ignore' });
	}
} catch {
	// Silently ignore if git is missing or repository configuration fails
}
