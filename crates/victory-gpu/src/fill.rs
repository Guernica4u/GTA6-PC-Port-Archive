//! A stand-in for the screen probes' ray activation the layer leaves out.
//!
//! Without the ray-tracing structure builder the layer leaves the title's
//! `CS_ScreenProbes_ActivateRays` out (`pipeline::make`): run, it sends rays
//! that find nothing and light every probe with the sky (white water, a black
//! sun). Left out, what it writes (the probes' records, the list of rays and
//! its count) keeps what the memory held before, and the passes after it
//! read that: hard-edged blocks on the water and in the sky.
//!
//! With `VICTORY_ACTIVATE_ZERO=1` the buffers it writes are zeroed on the
//! card in its place instead: no rays, no probe light, and the stand-in for
//! the indirect light (`crate::ambient`) raises what is missing to its
//! estimate.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::descriptor::View;
use crate::object::This;

/// Whether the stand-in is on (`VICTORY_ACTIVATE_ZERO=1`; off by default).
pub(crate) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_ACTIVATE_ZERO").is_some_and(|v| v == "1"))
}

/// Bytes of an element of a typed buffer view in `format`, if known.
fn format_bytes(format: u32) -> Option<u64> {
    Some(match format {
        1..=4 => 16,
        5..=8 => 12,
        9..=22 => 8,
        23..=47 => 4,
        48..=59 => 2,
        60..=65 => 1,
        _ => return None,
    })
}

/// The title memory a view of unordered access names, as (address, bytes):
/// the buffer's elements the view covers, and its counter's four bytes.
/// Empty for views of textures and for those it cannot tell.
pub(crate) fn ranges(view: &View) -> Vec<(u64, u64)> {
    let (base, size, desc, counter) = match view {
        View::Unordered(r, counter, desc) => {
            let Some((data, size)) = crate::resource::buffer_bytes(*r as This) else {
                return Vec::new();
            };
            (data as u64, size, *desc, *counter)
        }
        // Placed memory: the resource description, then the view's with the
        // memory's address in words 10 and 11.
        View::Placed(true, resource, desc) => {
            const BUFFER: u32 = 1;
            if resource[0] != BUFFER {
                return Vec::new();
            }
            let width = u64::from(resource[4]) | u64::from(resource[5]) << 32;
            let base = u64::from(desc[10]) | u64::from(desc[11]) << 32;
            let mut plain = [0u32; 10];
            plain.copy_from_slice(&desc[..10]);
            (base, width, Some(plain), 0)
        }
        _ => return Vec::new(),
    };
    let Some(d) = desc else {
        return vec![(base, size)];
    };
    const VIEW_BUFFER: u32 = 1;
    if d[1] != VIEW_BUFFER {
        return Vec::new();
    }
    let first = u64::from(d[2]) | u64::from(d[3]) << 32;
    let count = u64::from(d[4]);
    let stride = u64::from(d[5]);
    let raw = d[8] & 1 != 0;
    let each = if stride > 0 {
        stride
    } else if raw {
        4
    } else {
        match format_bytes(d[0]) {
            Some(n) => n,
            None => return Vec::new(),
        }
    };
    let mut out = Vec::new();
    if let (Some(start), Some(len)) = (first.checked_mul(each), count.checked_mul(each))
        && start < size
    {
        out.push((base + start, len.min(size - start)));
    }
    let counter_at = u64::from(d[6]) | u64::from(d[7]) << 32;
    if counter != 0
        && let Some((data, size)) = crate::resource::buffer_bytes(counter as This)
        && counter_at + 4 <= size
    {
        out.push((data as u64 + counter_at, 4));
    }
    out
}

/// With the ray activation run (`VICTORY_ACTIVATE_RAYS=1`):
/// `VICTORY_ACTIVATE_COUNTS=<dwords>[,...]` zeroes, right after it, the
/// buffers it writes at those places of its table (`1`: 112, the tiles' ray
/// counts its last `buffer_atomic_add` counts up), so that its tiles are set
/// up but none asks for rays.
pub(crate) fn counts() -> Option<&'static [u32]> {
    static SET: OnceLock<Vec<u32>> = OnceLock::new();
    let set = SET.get_or_init(|| {
        let v = std::env::var("VICTORY_ACTIVATE_COUNTS").unwrap_or_default();
        match v.trim() {
            "" | "0" => Vec::new(),
            "1" => vec![112],
            v => v.split(',').filter_map(|n| n.trim().parse().ok()).collect(),
        }
    });
    (!set.is_empty()).then_some(set.as_slice())
}

/// Which bytes of each buffer [`counts`] names are zeroed
/// (`VICTORY_ACTIVATE_COUNT_BYTES=<from>,<bytes>`; unset: all). The tiles'
/// counts are 24 bytes, two sets of dispatch arguments: `0,12` the first,
/// `12,12` the second.
pub(crate) fn count_bytes(range: (u64, u64)) -> Option<(u64, u64)> {
    static SET: OnceLock<Option<(u64, u64)>> = OnceLock::new();
    let set = *SET.get_or_init(|| {
        let v = std::env::var("VICTORY_ACTIVATE_COUNT_BYTES").ok()?;
        let mut n = v.split(',').map(|n| n.trim().parse::<u64>().ok());
        Some((n.next()??, n.next()??))
    });
    let Some((from, bytes)) = set else {
        return Some(range);
    };
    (from < range.1).then(|| (range.0 + from, bytes.min(range.1 - from)))
}

/// Logs each buffer the ray activation writes, once: its table place and
/// the memory it names.
pub(crate) fn tell(slot: u8, offset: u32, ranges: &[(u64, u64)]) {
    static TOLD: std::sync::Mutex<Vec<(u8, u32)>> = std::sync::Mutex::new(Vec::new());
    let mut told = TOLD.lock().unwrap_or_else(|e| e.into_inner());
    if told.contains(&(slot, offset)) || told.len() > 64 {
        return;
    }
    told.push((slot, offset));
    let bytes: Vec<u64> = ranges.iter().map(|r| r.1).collect();
    tracing::info!(slot, offset_dwords = offset, ?bytes, "a buffer the ray activation writes");
}

static ZEROED: AtomicU64 = AtomicU64::new(0);
static TIMES: AtomicU64 = AtomicU64::new(0);

/// One left-out dispatch: bytes zeroed, views that could not be.
pub(crate) fn note(zeroed: u64, missed: u32) {
    let n = TIMES.fetch_add(1, Ordering::Relaxed);
    let all = ZEROED.fetch_add(zeroed, Ordering::Relaxed) + zeroed;
    if n < 4 || n.is_power_of_two() {
        tracing::info!(
            times = n + 1,
            zeroed,
            missed,
            all_bytes = all,
            "left-out ray activation: what it writes zeroed in its place"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_bytes(2), Some(16)); // R32G32B32A32_FLOAT
        assert_eq!(format_bytes(10), Some(8)); // R16G16B16A16_FLOAT
        assert_eq!(format_bytes(28), Some(4)); // R8G8B8A8_UNORM
        assert_eq!(format_bytes(42), Some(4)); // R32_UINT
        assert_eq!(format_bytes(0), None);
    }
}
