use super::*;
use windows::Win32::System::ProcessStatus::{PSAPI_WORKING_SET_EX_INFORMATION, QueryWorkingSetEx};
use windows::Win32::System::Threading::GetCurrentProcess;

/// Whether the page holding `at` is resident.
fn resident(at: *const u8) -> bool {
    let mut info = PSAPI_WORKING_SET_EX_INFORMATION { VirtualAddress: at as *mut _, ..Default::default() };
    // SAFETY: one entry, sized for the call.
    unsafe { QueryWorkingSetEx(GetCurrentProcess(), (&raw mut info).cast(), size_of_val(&info) as u32).unwrap() };
    // SAFETY: Flags covers the whole union; bit 0 is Valid.
    unsafe { info.VirtualAttributes.Flags & 1 != 0 }
}

#[test]
fn a_stored_snapshot_releases_the_scratch_pages_past_its_bytes() {
    // Upload compares every texture's mips through one scratch buffer, which
    // grows to the largest mip it has held and keeps those pages touched. A
    // changed small mip then takes that buffer as its snapshot.
    let mut scratch = vec![7u8; 16 << 20];
    scratch.truncate(4096);
    let tail = unsafe { scratch.as_ptr().add(8 << 20) };
    assert!(resident(tail));
    let mut contents = Contents { previous: vec![Vec::new()], ..Default::default() };
    contents.store(0, scratch);
    let kept = &contents.previous[0];
    assert_eq!(kept.len(), 4096);
    assert!(kept.iter().all(|&byte| byte == 7));
    assert!(!resident(tail), "the scratch pages past the snapshot's bytes stay resident");
}

#[test]
fn a_released_snapshot_can_grow_back() {
    let mut scratch = vec![7u8; 16 << 20];
    scratch.truncate(4096);
    let mut contents = Contents { previous: vec![Vec::new()], ..Default::default() };
    contents.store(0, scratch);
    // Reused as scratch for a larger mip, the buffer refills the released pages.
    let mut reused = contents.store(0, Vec::new());
    reused.resize(16 << 20, 9);
    assert!(reused[..4096].iter().all(|&byte| byte == 7));
    assert!(reused[4096..].iter().all(|&byte| byte == 9));
}

#[test]
fn store_returns_the_replaced_snapshot_for_reuse() {
    let mut contents = Contents { previous: vec![vec![1u8; 64], vec![2u8; 32]], ..Default::default() };
    let old = contents.store(1, vec![3u8; 32]);
    assert_eq!(old, vec![2u8; 32]);
    assert_eq!(contents.previous, [vec![1u8; 64], vec![3u8; 32]]);
}
