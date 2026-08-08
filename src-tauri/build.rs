use std::process::Command;

fn git_path(relative: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", relative])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    let commit_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    println!("cargo:rustc-env=BUILD_COMMIT_HASH={}", commit_hash);

    // `.git/HEAD` only changes on checkout/branch-switch (it's a symbolic ref
    // like `ref: refs/heads/main`); new commits on the same branch update the
    // ref file it points to instead. Watch both so the embedded commit hash
    // stays fresh across ordinary commits, not just branch switches. Resolve
    // paths via `git rev-parse --git-path` since worktrees keep them outside
    // the worktree's own `.git`.
    if let Some(head_path) = git_path("HEAD") {
        println!("cargo:rerun-if-changed={}", head_path);

        if let Some(ref_name) = std::fs::read_to_string(&head_path)
            .ok()
            .and_then(|contents| contents.trim().strip_prefix("ref: ").map(String::from))
        {
            if let Some(ref_path) = git_path(&ref_name) {
                println!("cargo:rerun-if-changed={}", ref_path);
            }
        }
    }

    tauri_build::build();
}
