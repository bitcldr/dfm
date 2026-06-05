//! Embeds a build-time revision string into the binary via `DFM_REVISION`.
//!
//! The revision is `git describe --tags --always --dirty` (e.g. `v0.3.0`,
//! `v0.3.0-2-gabc1234`, or `abc1234-dirty`). When git is unavailable or this is
//! not a repository, it falls back to `dev`. The build is re-run when HEAD or
//! the index changes so the value stays current.

use std::process::Command;

fn main() {
    let revision = git_describe().unwrap_or_else(|| "dev".to_string());
    println!("cargo:rustc-env=DFM_REVISION={revision}");

    // Re-run when the commit or working-tree state changes.
    for path in [".git/HEAD", ".git/index"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=DFM_REVISION");
}

fn git_describe() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}
