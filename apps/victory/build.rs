//! Records the commit this build was made from (#104).
//!
//! A test report that names a version but not the commit it was built from
//! cannot be placed against a branch: `0.1.0` says nothing after the first
//! merge. The commit is read from git when the build happens in a checkout;
//! a build from a tarball has none and says so by saying nothing.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // `.git/HEAD` moves on every checkout, and `refs` under it on every
    // commit: rerun the build when either does.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let package = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    match commit() {
        Some(commit) => {
            println!("cargo:rustc-env=VICTORY_COMMIT={commit}");
            println!("cargo:rustc-env=VICTORY_VERSION={package} ({commit})");
        }
        // No commit to name: the version stands alone, as before.
        None => println!("cargo:rustc-env=VICTORY_VERSION={package}"),
    }
}

/// The commit this checkout is at, seven hex digits, as git prints them.
fn commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!commit.is_empty()).then_some(commit)
}
