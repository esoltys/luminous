# Security Policy

First off, thanks for helping keep Luminous safe! As an open-source project, I rely on community members like you to catch issues and keep our users secure.

## Supported Versions

If you're using an older version, we recommend upgrading to the [latest release](https://github.com/esoltys/luminous/releases/latest) to get fixes.

## Reporting a Vulnerability

If you think you've found a security bug in Luminous, **please don't create a public GitHub issue or post about it in discussions.** We want to fix the problem before it gets out into the wild!

### How to reach out privately

1. **GitHub Security Advisories (Preferred):** Click the **Security** tab at the top of this repository, then click **Report a vulnerability** to open a private report with the maintainers.
2. **Direct Contact:** If you prefer you can find my social accounts at https://esoltys.dev so you can DM me as well.

### What to include

To help me fix things quickly, try to share:
- A quick overview of what the bug is and where it lives.
- Steps to reproduce it or what you were doing when you experienced the problem.

## What happens next?

- **Fast Acknowledgement:** I will reply to let you know we received your report (usually within 24–48 hours).
- **Collaboration:** I'll work on a fix privately and keep you in the loop.
- **Credit:** Once a fix is ready and released, we'll gladly give you credit in our release notes (unless you prefer to stay anonymous).

Thank you for making Luminous better for everyone!

## Automated Security & Supply Chain Monitoring

To ensure the security and integrity of Luminous and its dependencies, we maintain automated security checks in CI:

- **Cargo Security Auditing (`cargo-audit`)**: Scans `Cargo.lock` against the [RustSec Advisory Database](https://rustsec.org/) on every commit, pull request, and weekly schedule.
- **CodeQL Analysis**: Continuous static application security testing (SAST) across Rust, JavaScript/TypeScript, and GitHub Actions code.
- **Dependabot Security & Version Updates**: Automated dependency tracking and security alerts for Rust (`cargo`) and frontend (`npm`) packages.

