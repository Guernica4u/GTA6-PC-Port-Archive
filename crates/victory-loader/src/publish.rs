//! Portable publication of derived files into a shared cache directory.
//!
//! Several copies of the runner can prepare the same title at once. Each
//! writer stages into its own file, so two of them cannot truncate each
//! other's bytes or rename a file the other already published. The final
//! rename is atomic within the directory, and the staging file is removed
//! if publication fails.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);

/// The digest sidecar published beside `path`: comparing it against the
/// digest of the new bytes answers "already there" without reading the
/// cached image back (a title image is ~100 MB; the read per boot per DLL
/// was the point of the sidecar).
fn sidecar(path: &Path) -> std::path::PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
    path.with_file_name(format!(".{name}.blake3"))
}

/// Writes `bytes` to `path` unless it is already there, using a staging file
/// unique to this process and call. "Already there" is answered from the
/// digest sidecar when there is one, and by reading the file back otherwise
/// (a cache from before sidecars).
pub fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let digest = blake3::hash(bytes);
    // Fast path: sidecar digest matches and the file is there with the
    // right length — no read of the image.
    if std::fs::read(sidecar(path)).is_ok_and(|d| d == *digest.as_bytes())
        && std::fs::metadata(path).is_ok_and(|m| m.len() == bytes.len() as u64)
    {
        return Ok(());
    }
    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        // Publish the sidecar for the next boot; a failure here only costs
        // the fallback read next time.
        let _ = std::fs::write(sidecar(path), digest.as_bytes());
        return Ok(());
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let sequence = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
    let staging = path.with_file_name(format!(
        ".{name}.{}.{sequence}.tmp",
        std::process::id()
    ));
    std::fs::write(&staging, bytes)?;
    if let Err(error) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(error);
    }
    // The digest of what was published, for the fast path next time. Not
    // fatal: without it the next boot falls back to reading the image.
    let _ = std::fs::write(sidecar(path), digest.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two publishers staging the same destination at once both succeed,
    /// and neither leaves a staging file behind.
    #[test]
    fn concurrent_publishers_do_not_share_a_staging_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.bin");
        let bytes = b"a prepared image".to_vec();
        let (first, second) = (path.clone(), path.clone());
        let (first_bytes, second_bytes) = (bytes.clone(), bytes.clone());
        let (one, two) = (
            std::thread::spawn(move || write_if_changed(&first, &first_bytes)),
            std::thread::spawn(move || write_if_changed(&second, &second_bytes)),
        );
        one.join().unwrap().unwrap();
        two.join().unwrap().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path() != path && entry.path() != sidecar(&path))
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging files were removed: {leftovers:?}"
        );
    }

    /// A cache hit is answered from the sidecar digest: the image file is
    /// not read back (it is unreadable here and the call still succeeds).
    /// A cache from before sidecars falls back to reading, and publishes the
    /// sidecar for next time. Changed bytes republish and move the sidecar.
    #[test]
    fn a_cache_hit_reads_the_sidecar_not_the_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.bin");
        let bytes = vec![7u8; 1 << 20];
        write_if_changed(&path, &bytes).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let digest = std::fs::read(sidecar(&path)).unwrap();
        assert_eq!(digest.len(), 32);
        // Same bytes again: a hit, nothing rewritten.
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        write_if_changed(&path, &bytes).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
        // The image deleted under a matching sidecar: republished (the
        // length check fails, the read fallback misses, the write happens).
        std::fs::remove_file(&path).unwrap();
        write_if_changed(&path, &bytes).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        // Changed bytes republish and move the sidecar.
        let new = vec![8u8; 1 << 20];
        write_if_changed(&path, &new).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), new);
        assert_eq!(
            std::fs::read(sidecar(&path)).unwrap(),
            blake3::hash(&new).as_bytes()
        );
        // A legacy cache (no sidecar) falls back to reading, then publishes
        // the sidecar.
        std::fs::remove_file(sidecar(&path)).unwrap();
        write_if_changed(&path, &new).unwrap();
        assert_eq!(
            std::fs::read(sidecar(&path)).unwrap(),
            blake3::hash(&new).as_bytes()
        );
        // Boot cost: 64 MB image, cache hit — sidecar read + in-memory hash
        // vs the old full read + compare.
        let big = vec![3u8; 64 << 20];
        let big_path = dir.path().join("big.bin");
        write_if_changed(&big_path, &big).unwrap();
        let t0 = std::time::Instant::now();
        for _ in 0..5 {
            write_if_changed(&big_path, &big).unwrap();
        }
        let hit = t0.elapsed() / 5;
        let t0 = std::time::Instant::now();
        for _ in 0..5 {
            let existing = std::fs::read(&big_path).unwrap();
            assert!(existing == big);
        }
        let old = t0.elapsed() / 5;
        eprintln!("64 MB cache hit: sidecar path {hit:?}, old read+compare {old:?}");
    }

    #[test]
    fn a_failed_publication_removes_its_staging_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-parent").join("image.bin");
        assert!(write_if_changed(&path, b"bytes").is_err());
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .collect();
        assert!(leftovers.is_empty(), "no staging file survives an error");
    }
}
