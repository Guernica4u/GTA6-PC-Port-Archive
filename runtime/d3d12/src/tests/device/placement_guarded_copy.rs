use super::{available, copy, recover};
use windows::Win32::System::Diagnostics::Debug::CONTEXT;
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_NOACCESS, PAGE_PROTECTION_FLAGS,
    PAGE_READONLY, PAGE_READWRITE, VirtualAlloc, VirtualFree, VirtualProtect,
};

const PAGE: usize = 4096;

struct Pages(*mut u8);

impl Pages {
    fn new(count: usize) -> Self {
        // SAFETY: a fresh private allocation.
        let base = unsafe { VirtualAlloc(None, count * PAGE, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        assert!(!base.is_null());
        Self(base.cast())
    }

    fn bytes(&self, range: std::ops::Range<usize>) -> &[u8] {
        // SAFETY: tests only read pages they left committed and readable.
        unsafe { std::slice::from_raw_parts(self.0.add(range.start), range.len()) }
    }
}

impl Drop for Pages {
    fn drop(&mut self) {
        // SAFETY: the allocation belongs to this test.
        unsafe { VirtualFree(self.0.cast(), 0, MEM_RELEASE).unwrap() };
    }
}

#[test]
fn copies_into_writable_pages() {
    let pages = Pages::new(2);
    let source: Vec<u8> = (0..PAGE + 100).map(|i| (i * 7 + 1) as u8).collect();
    // SAFETY: the destination range is committed and writable.
    assert!(unsafe { copy(pages.0.add(50), &source) });
    assert_eq!(pages.bytes(50..50 + source.len()), source);
    assert!(pages.bytes(0..50).iter().all(|&b| b == 0));
    // An empty copy stores nothing, even at an address that is not mapped.
    assert!(unsafe { copy(core::ptr::null_mut(), &[]) });
}

#[test]
fn a_fault_in_the_destination_fails_the_copy() {
    assert!(available());
    let source = vec![0x5au8; 2 * PAGE];
    for change in 0..3 {
        let pages = Pages::new(3);
        let second = unsafe { pages.0.add(PAGE) };
        let mut old = PAGE_PROTECTION_FLAGS::default();
        // The middle page stops being writable after any check a caller made.
        unsafe {
            match change {
                0 => VirtualProtect(second.cast(), PAGE, PAGE_NOACCESS, &mut old).unwrap(),
                1 => VirtualProtect(second.cast(), PAGE, PAGE_READONLY, &mut old).unwrap(),
                _ => VirtualFree(second.cast(), PAGE, MEM_DECOMMIT).unwrap(),
            }
        }
        // SAFETY: the fault on the second page is the behaviour under test.
        assert!(!unsafe { copy(pages.0.add(PAGE - 16), &source) }, "change {change}");
        // Bytes before the fault were stored, and the last page was not reached.
        assert!(pages.bytes(PAGE - 16..PAGE).iter().all(|&b| b == 0x5a));
        assert!(pages.bytes(2 * PAGE..3 * PAGE).iter().all(|&b| b == 0));
        // The thread keeps working after a recovered fault.
        assert!(unsafe { copy(pages.0, &source[..PAGE]) });
    }
}

#[test]
fn only_a_fault_at_the_store_is_recovered() {
    let mut context = CONTEXT::default();
    context.Rip = copies_into_writable_pages as fn() as usize as u64;
    let before = context.Rip;
    assert!(!recover(&mut context));
    assert_eq!(context.Rip, before);
}
