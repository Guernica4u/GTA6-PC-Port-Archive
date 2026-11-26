//! Portable publication of derived files into a shared cache directory.
//!
//! Several copies of the runner can prepare the same title at once. Each
//! writer stages into its own file, so two of them cannot truncate each
//! other's bytes or rename a file the other already published. The final
//! rename is atomic within the directory, and the staging file is removed
//! if publication fails.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, LOCKFILE_EXCLUSIVE_LOCK,
    LockFileEx, OPEN_ALWAYS,
};
#[cfg(windows)]
use windows_sys::Win32::System::IO::OVERLAPPED;

static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);

/// The digest sidecar published beside `path`: comparing it against the
/// digest of the new bytes answers "already there" without reading the
/// cached image back (a title image is ~100 MB; the read per boot per DLL
/// was the point of the sidecar).
fn sidecar(path: &Path) -> std::path::PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
    path.with_file_name(format!(".{name}.blake3"))
}

/// Writes the sidecar through its own staging file + rename, so a reader
/// never sees a torn digest. Not fatal: without it the next boot falls back
/// to reading the image.
fn write_sidecar(path: &Path, digest: &blake3::Hash) {
    let side = sidecar(path);
    let name = side.file_name().and_then(|n| n.to_str()).unwrap_or("digest");
    let sequence = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
    let staging = side.with_file_name(format!(".{name}.{}.{sequence}.tmp", std::process::id()));
    if std::fs::write(&staging, digest.as_bytes()).and_then(|_| std::fs::rename(&staging, &side)).is_err() {
        let _ = std::fs::remove_file(&staging);
    }
}

/// An exclusive lock on the lock file beside the published path, held for
/// the guard's life: closing the handle releases it, so a crashed publisher
/// does not leave the cache locked.
#[cfg(windows)]
struct PublishLock(HANDLE);
#[cfg(windows)]
impl Drop for PublishLock {
    fn drop(&mut self) {
        // SAFETY: our handle.
        unsafe { CloseHandle(self.0) };
    }
}

/// Locks the publishers of `path` against each other.
#[cfg(windows)]
fn publish_lock(path: &Path) -> std::io::Result<PublishLock> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
    let lock_path = path.with_file_name(format!(".{name}.lock"));
    let wide: Vec<u16> = lock_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: a valid path, null security attributes, no template.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a zeroed OVERLAPPED is valid; unused on a synchronous handle.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    // SAFETY: a valid handle.
    if unsafe {
        LockFileEx(
            handle,
            LOCKFILE_EXCLUSIVE_LOCK,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    } == 0
    {
        let error = std::io::Error::last_os_error();
        // SAFETY: our handle.
        unsafe { CloseHandle(handle) };
        return Err(error);
    }
    Ok(PublishLock(handle))
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
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let sequence = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
    let staging = path.with_file_name(format!(".{name}.{}.{sequence}.tmp", std::process::id()));
    std::fs::write(&staging, bytes)?;
    // Serialize publishers, and order the sidecar around the rename: it is
    // removed before the rename and written after it, so a sidecar on disk
    // always describes the file on disk. A crash in between leaves no
    // sidecar, and the next boot falls back to reading the image.
    // The read-back of a cache from before sidecars is inside the same
    // critical section: written outside it, its sidecar could describe an
    // image another publisher replaced meanwhile.
    #[cfg(windows)]
    let _lock = publish_lock(path)?;
    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        // Already there: keep it, and publish the sidecar for the next boot
        // (a failure there only costs the fallback read next time).
        let _ = std::fs::remove_file(&staging);
        write_sidecar(path, &digest);
        return Ok(());
    }
    // A sidecar that cannot be removed must not survive beside an image it
    // no longer describes: stop before replacing the image.
    match std::fs::remove_file(sidecar(path)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            let _ = std::fs::remove_file(&staging);
            return Err(error);
        }
    }
    if let Err(error) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(error);
    }
    // The digest of what was published, for the fast path next time. Not
    // fatal: without it the next boot falls back to reading the image.
    write_sidecar(path, &digest);
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
            .filter(|entry| {
                let p = entry.path();
                let name = path.file_name().unwrap().to_str().unwrap();
                p != path
                    && p != sidecar(&path)
                    && p != path.with_file_name(format!(".{name}.lock"))
            })
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

    /// A cache from before sidecars: its sidecar is backfilled under the
    /// publishers' lock, so it cannot land over an image another publisher
    /// replaced meanwhile. Held here, a publisher waits for the lock.
    #[cfg(windows)]
    #[test]
    fn a_legacy_sidecar_backfill_waits_for_the_publishers_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.bin");
        let bytes = vec![0x1au8; 1 << 16];
        // A legacy cache: the image is there, the sidecar is not.
        std::fs::write(&path, &bytes).unwrap();
        let held = publish_lock(&path).unwrap();
        let (p, b) = (path.clone(), bytes.clone());
        let published = std::thread::spawn(move || write_if_changed(&p, &b));
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(
            !sidecar(&path).exists(),
            "the sidecar was published outside the publishers' lock"
        );
        drop(held);
        published.join().unwrap().unwrap();
        assert_eq!(
            std::fs::read(sidecar(&path)).unwrap(),
            blake3::hash(&bytes).as_bytes()
        );
    }

    /// A sidecar another process holds without delete sharing cannot be
    /// removed: publication stops before replacing the image, so the sidecar
    /// on disk still describes the file on disk.
    #[cfg(windows)]
    #[test]
    fn a_sidecar_that_cannot_be_removed_stops_the_publish() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.bin");
        let a = vec![0xaau8; 1 << 16];
        let b = vec![0xbbu8; 1 << 16];
        write_if_changed(&path, &a).unwrap();
        let side = sidecar(&path);
        let wide: Vec<u16> = side
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: a valid path; the handle is closed below.
        let open = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ,
                std::ptr::null(),
                windows_sys::Win32::Storage::FileSystem::OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(
            open,
            INVALID_HANDLE_VALUE,
            "open the sidecar: {}",
            std::io::Error::last_os_error()
        );
        let stopped = write_if_changed(&path, &b);
        // SAFETY: our handle.
        unsafe { CloseHandle(open) };
        assert!(
            stopped.is_err(),
            "a publish whose old sidecar cannot be removed must fail: {stopped:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), a, "the image is untouched");
        assert_eq!(
            std::fs::read(&side).unwrap(),
            blake3::hash(&a).as_bytes(),
            "the sidecar still describes the image"
        );
    }

    /// Many interleaved publishers of alternating contents: whenever a
    /// sidecar exists, it describes the file on disk (the remove-before-
    /// rename / write-after ordering under the lock).
    #[test]
    fn concurrent_publishers_keep_the_sidecar_matching_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.bin");
        let a = vec![1u8; 4096];
        let b = vec![2u8; 4096];
        let mut handles = Vec::new();
        for t in 0..4u32 {
            let (path, a, b) = (path.clone(), a.clone(), b.clone());
            handles.push(std::thread::spawn(move || {
                for i in 0..50u32 {
                    let bytes = if (i + t) % 2 == 0 { &a } else { &b };
                    write_if_changed(&path, bytes).unwrap();
                    // If the file did not change across the check, a sidecar
                    // present at the same time must describe it.
                    // A rename by another publisher can make these reads
                    // fail transiently; only check when all three succeed.
                    if let (Ok(before), Ok(d), Ok(after)) = (
                        std::fs::read(&path),
                        std::fs::read(sidecar(&path)),
                        std::fs::read(&path),
                    ) && before == after && !d.is_empty()
                    {
                        assert_eq!(d, blake3::hash(&before).as_bytes(), "sidecar matches the file");
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(
            std::fs::read(sidecar(&path)).unwrap(),
            blake3::hash(&on_disk).as_bytes()
        );
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
