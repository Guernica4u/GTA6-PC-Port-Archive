//! Records the commit this build was made from (#104).
//!
//! A test report that names a version but not the commit it was built from
//! cannot be placed against a branch: `0.1.0` says nothing after the first
//! merge. The commit is read from git when the build happens in a checkout;
//! a build from a tarball has none and says so by saying nothing.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // What `HEAD` is, not only what `HEAD` says: a commit on the branch
    // writes the branch's own file and leaves `HEAD`'s text alone, so
    // watching `HEAD` alone kept the first commit ever built in the version
    // line while the branch moved on.
    for path in watched() {
        println!("cargo:rerun-if-changed={path}");
    }
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

/// The files that decide which commit `HEAD` is, those that exist.
///
/// A watched path that does not exist makes cargo rerun the script every
/// time, so `packed-refs` is watched only when a packed checkout has it.
fn watched() -> Vec<String> {
    let mut paths = vec!["../../.git/HEAD".to_owned()];
    if let Ok(head) = std::fs::read_to_string("../../.git/HEAD")
        && let Some(reference) = head.trim().strip_prefix("ref: ")
    {
        // The branch file exists in an ordinary checkout and is where a
        // commit lands; in a packed checkout the ref lives in the pack.
        let branch = format!("../../.git/{reference}");
        let packed = "../../.git/packed-refs".to_owned();
        if std::path::Path::new(&branch).exists() {
            paths.push(branch);
        } else if std::path::Path::new(&packed).exists() {
            paths.push(packed);
        }
    }
    paths
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
