use super::{query_region, write_mapped};
use windows::Win32::System::Memory::{
    GetWriteWatch, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_NOACCESS,
    PAGE_PROTECTION_FLAGS, PAGE_READONLY, PAGE_READWRITE, VIRTUAL_ALLOCATION_TYPE, VirtualAlloc,
    VirtualFree, VirtualProtect,
};

const PAGE: usize = 4096;
/// Allocation type that records CPU writes for GetWriteWatch.
const MEM_WRITE_WATCH: VIRTUAL_ALLOCATION_TYPE = VIRTUAL_ALLOCATION_TYPE(0x0020_0000);

struct Pages(*mut u8);

impl Pages {
    fn new(count: usize, extra: VIRTUAL_ALLOCATION_TYPE) -> Self {
        // SAFETY: a fresh private allocation.
        let base = unsafe { VirtualAlloc(None, count * PAGE, MEM_COMMIT | MEM_RESERVE | extra, PAGE_READWRITE) };
        assert!(!base.is_null());
        Self(base.cast())
    }

    fn protect(&self, page: usize, protection: PAGE_PROTECTION_FLAGS) {
        let mut old = PAGE_PROTECTION_FLAGS::default();
        // SAFETY: the page belongs to this allocation.
        unsafe { VirtualProtect(self.0.add(page * PAGE).cast(), PAGE, protection, &mut old).unwrap() };
    }

    fn bytes(&self, range: std::ops::Range<usize>) -> &[u8] {
        // SAFETY: tests only read pages they left readable.
        unsafe { std::slice::from_raw_parts(self.0.add(range.start), range.len()) }
    }

    /// Pages of a write-watch allocation that record a CPU write.
    fn watched_writes(&self, count: usize) -> usize {
        let mut addresses = [core::ptr::null_mut(); 16];
        let (mut found, mut granularity) = (addresses.len(), 0u32);
        // SAFETY: the outputs are locals of the stated capacity.
        let status = unsafe {
            GetWriteWatch(0, self.0.cast(), count * PAGE, Some(addresses.as_mut_ptr()), Some(&mut found), Some(&mut granularity))
        };
        assert_eq!(status, 0);
        found
    }
}

impl Drop for Pages {
    fn drop(&mut self) {
        // SAFETY: the allocation belongs to this test.
        unsafe { VirtualFree(self.0.cast(), 0, MEM_RELEASE).unwrap() };
    }
}

fn publish(pages: &Pages, bytes: &[u8], range: std::ops::Range<usize>) -> windows::core::Result<()> {
    let base = pages.0.addr();
    let mut region = query_region(base + range.start)?;
    write_mapped(base, bytes, range, &mut region)
}

#[test]
fn plain_data_pages_take_a_store() {
    assert!(crate::survey::install_handler());
    let pages = Pages::new(3, VIRTUAL_ALLOCATION_TYPE(0));
    assert!(query_region(pages.0.addr()).unwrap().2);
    let bytes: Vec<u8> = (0..3 * PAGE).map(|i| (i % 251) as u8 + 1).collect();
    publish(&pages, &bytes, 100..2 * PAGE + 7).unwrap();
    assert_eq!(pages.bytes(100..2 * PAGE + 7), &bytes[100..2 * PAGE + 7]);
    assert!(pages.bytes(0..100).iter().all(|&b| b == 0));
    assert!(pages.bytes(2 * PAGE + 7..3 * PAGE).iter().all(|&b| b == 0));
}

#[test]
fn a_write_watch_region_keeps_the_native_write() {
    let pages = Pages::new(2, MEM_WRITE_WATCH);
    assert!(!query_region(pages.0.addr()).unwrap().2);
    // Asking whether the region is watched neither adds nor clears a record.
    assert_eq!(pages.watched_writes(2), 0);
    let bytes = vec![0x77u8; 2 * PAGE];
    publish(&pages, &bytes, 8..PAGE + 8).unwrap();
    assert_eq!(pages.bytes(8..PAGE + 8), &bytes[8..PAGE + 8]);
    // A CPU write by the title is still recorded afterwards.
    // SAFETY: the page is committed and writable.
    unsafe { pages.0.add(PAGE + 100).write_volatile(1) };
    assert!(!query_region(pages.0.addr()).unwrap().2);
    assert!(pages.watched_writes(2) >= 1);
}

#[test]
fn closed_and_read_only_pages_are_not_stored_into() {
    let pages = Pages::new(3, VIRTUAL_ALLOCATION_TYPE(0));
    let bytes = vec![0x33u8; 3 * PAGE];
    // A page with no access has no guest bytes to publish and is skipped.
    pages.protect(1, PAGE_NOACCESS);
    publish(&pages, &bytes, 0..3 * PAGE).unwrap();
    assert!(pages.bytes(0..PAGE).iter().all(|&b| b == 0x33));
    assert!(pages.bytes(2 * PAGE..3 * PAGE).iter().all(|&b| b == 0x33));
    pages.protect(1, PAGE_READONLY);
    assert!(pages.bytes(PAGE..2 * PAGE).iter().all(|&b| b == 0));
    // A read-only page is the title's to protect: publication fails instead.
    assert!(publish(&pages, &bytes, 0..3 * PAGE).is_err());
    assert!(pages.bytes(PAGE..2 * PAGE).iter().all(|&b| b == 0));
}
