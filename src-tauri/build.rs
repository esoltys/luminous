use std::process::Command;

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
    println!("cargo:rerun-if-changed=.git/HEAD");

    tauri_build::build();
}
