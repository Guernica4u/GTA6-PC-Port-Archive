//! Store into guest pages that the title can unmap or protect at any moment.
//!
//! The title owns these pages, so a check made before a store can be stale by
//! the time the store runs. A fault inside the copy is therefore turned into a
//! failed copy by the provider's vectored exception handler, which resumes at
//! `tuxbox_guarded_copy_fault` when the faulting instruction is the store.
use windows::Win32::System::Diagnostics::Debug::CONTEXT;

// Windows x64: rcx = destination, rdx = source, r8 = length. RSI and RDI are
// callee-saved and kept in volatile registers, so the routine never moves the
// stack and needs no unwind data. The direction flag is clear on entry.
core::arch::global_asm!(
    ".globl tuxbox_guarded_copy",
    ".globl tuxbox_guarded_copy_store",
    ".globl tuxbox_guarded_copy_fault",
    "tuxbox_guarded_copy:",
    "mov r9, rsi",
    "mov r10, rdi",
    "mov rdi, rcx",
    "mov rsi, rdx",
    "mov rcx, r8",
    "tuxbox_guarded_copy_store:",
    "rep movsb",
    "mov rsi, r9",
    "mov rdi, r10",
    "mov eax, 1",
    "ret",
    "tuxbox_guarded_copy_fault:",
    "mov rsi, r9",
    "mov rdi, r10",
    "xor eax, eax",
    "ret",
);

unsafe extern "system" {
    fn tuxbox_guarded_copy(destination: *mut u8, source: *const u8, bytes: usize) -> u32;
    static tuxbox_guarded_copy_store: u8;
    static tuxbox_guarded_copy_fault: u8;
}

/// Whether a fault in the copy is caught. Without the handler a stale check
/// would crash the title, so callers keep the operating system's write.
pub(super) fn available() -> bool {
    crate::survey::install_handler()
}

/// Copy `source` to `destination`. Returns false, with part of the range
/// possibly written, when a page could not be read or written.
///
/// # Safety
/// `source` must be readable for its whole length. `destination` may be any
/// address: it is the guest's, and the handler reports a fault on it.
pub(super) unsafe fn copy(destination: *mut u8, source: &[u8]) -> bool {
    // SAFETY: the routine touches only the two ranges and volatile registers.
    unsafe { tuxbox_guarded_copy(destination, source.as_ptr(), source.len()) != 0 }
}

/// Called by the vectored exception handler for a memory fault. Resumes a
/// faulting guarded copy at its failure return.
pub(crate) fn recover(context: &mut CONTEXT) -> bool {
    if context.Rip != (&raw const tuxbox_guarded_copy_store).addr() as u64 {
        return false;
    }
    context.Rip = (&raw const tuxbox_guarded_copy_fault).addr() as u64;
    true
}

#[cfg(test)]
#[path = "../../tests/device/placement_guarded_copy.rs"]
mod tests;
