//! Link settings for the Windows executable, and the commit it is built
//! from.

use std::path::Path;
use std::process::Command;

fn main() {
    // Threads created without a stack size take the executable's default.
    // Host graphics drivers compiling the title's large shaders on such
    // threads (vkd3d-proton's and the drivers' own, under Wine too) overflow
    // the linker's default of 1 MiB: reserve 16 MiB instead.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
    commit();
}

/// `VICTORY_COMMIT`: the short hash of the commit checked out, which test
/// and crash reports name; `unknown commit` without git or a checkout.
fn commit() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = Path::new(&manifest).join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let hash = git(&["rev-parse", "--short=7", "HEAD"]);
    println!(
        "cargo:rustc-env=VICTORY_COMMIT={}",
        hash.as_deref().unwrap_or("unknown commit")
    );
    // Built again when HEAD moves: a checkout, or a commit on the branch.
    let mut watched = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    watched.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for name in watched {
        if let Some(path) = git(&["rev-parse", "--git-path", &name]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
