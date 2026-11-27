//! Verified direct branches in a mapped title before its first instruction runs.

use std::ffi::c_void;

use anyhow::{Context, Result, ensure};
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEM_RELEASE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_READWRITE, VirtualFree,
    VirtualProtect,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::LoadedImage;

const CALL_LEN: usize = 5;
const PAGE_LEN: usize = 4096;
const RELAY_LEN: usize = 16;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;

/// One checked direct call. The replacement must have the original Win64 ABI.
pub struct StartupCall {
    pub call_rva: u32,
    pub original_rva: u32,
    pub replacement: *const c_void,
}

/// One checked direct tail jump. The caller's original Win64 frame is kept.
pub struct StartupTailJump {
    pub jump_rva: u32,
    pub original_rva: u32,
    pub replacement: *const c_void,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Opcode {
    Call = 0xe8,
    TailJump = 0xe9,
}

fn branch(from: usize, to: usize, opcode: Opcode) -> Option<[u8; CALL_LEN]> {
    let next = from.checked_add(CALL_LEN)?;
    let displacement = i32::try_from(to as i128 - next as i128).ok()?;
    let mut bytes = [opcode as u8; CALL_LEN];
    bytes[1..].copy_from_slice(&displacement.to_le_bytes());
    Some(bytes)
}

fn relay(to: *const c_void) -> [u8; RELAY_LEN] {
    let mut bytes = [0xcc; RELAY_LEN];
    // jmp qword ptr [rip + 0]: neither the argument registers nor the stack change.
    bytes[..6].copy_from_slice(&[0xff, 0x25, 0, 0, 0, 0]);
    bytes[6..14].copy_from_slice(&(to as u64).to_le_bytes());
    bytes
}

fn protect(address: usize, size: usize, protection: u32) -> std::io::Result<u32> {
    let mut old = 0;
    // SAFETY: the callers pass our committed relay memory or a mapped code page.
    let result = unsafe { VirtualProtect(address as *const c_void, size, protection, &mut old) };
    if result == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(old)
}

fn flush(address: usize, size: usize) -> std::io::Result<()> {
    // SAFETY: the callers pass our committed memory in the current process.
    if unsafe { FlushInstructionCache(GetCurrentProcess(), address as *const c_void, size) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

struct WritablePage {
    address: usize,
    original: Option<u32>,
}

impl WritablePage {
    fn open(address: usize) -> Result<Self> {
        let original = protect(address, PAGE_LEN, PAGE_EXECUTE_READWRITE)
            .context("making a title call page writable")?;
        Ok(Self {
            address,
            original: Some(original),
        })
    }

    fn restore(&mut self) -> Result<()> {
        if let Some(original) = self.original {
            protect(self.address, PAGE_LEN, original).context("restoring a title call page")?;
            self.original = None;
        }
        Ok(())
    }
}

impl Drop for WritablePage {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

struct RelayMemory(usize);

impl Drop for RelayMemory {
    fn drop(&mut self) {
        // SAFETY: this is the base of our otherwise unreferenced allocation.
        unsafe { VirtualFree(self.0 as *mut c_void, 0, MEM_RELEASE) };
    }
}

/// Redirect checked CALL instructions in this process. Neither input executable
/// nor the prepared image on disk is modified. Relays share the title lifetime.
///
/// # Safety
/// Title code must be stopped before its first instruction, and must never
/// start if this returns an error. Replacements must be live for the process
/// lifetime and callable with the corresponding original Win64 ABI.
pub unsafe fn redirect(image: &LoadedImage, sites: &[StartupCall]) -> Result<()> {
    // SAFETY: this path preserves the original checked CALL-only contract.
    unsafe { redirect_with_tail_jumps(image, sites, &[]) }
}

/// Redirect a batch of checked calls and tail jumps. Every opcode and exact
/// destination is validated before any title code is made writable.
///
/// # Safety
/// The title must be stopped before startup. The replacements must preserve
/// the corresponding branch ABI without running title code on error.
pub unsafe fn redirect_with_tail_jumps(
    image: &LoadedImage,
    calls: &[StartupCall],
    tail_jumps: &[StartupTailJump],
) -> Result<()> {
    let base = image.base();
    let in_code = |rva: u32, bytes: u32| {
        rva.checked_add(bytes).is_some_and(|end| {
            end <= image.info().size_of_image
                && image.info().sections.iter().any(|section| {
                    section.characteristics & IMAGE_SCN_MEM_EXECUTE != 0
                        && rva >= section.rva
                        && end - section.rva <= section.virtual_size.min(section.file_size)
                })
        })
    };
    let count = calls.len() + tail_jumps.len();
    ensure!(
        (1..=PAGE_LEN / RELAY_LEN).contains(&count),
        "invalid title branch count"
    );

    let mut plan = Vec::with_capacity(count);
    let calls = calls.iter().map(|site| {
        (
            site.call_rva,
            site.original_rva,
            site.replacement,
            Opcode::Call,
        )
    });
    let tail_jumps = tail_jumps.iter().map(|site| {
        (
            site.jump_rva,
            site.original_rva,
            site.replacement,
            Opcode::TailJump,
        )
    });
    for (rva, original_rva, replacement, opcode) in calls.chain(tail_jumps) {
        ensure!(
            in_code(rva, CALL_LEN as u32) && in_code(original_rva, 1) && !replacement.is_null(),
            "title branch or callback outside executable code at {rva:#x}"
        );
        plan.push((
            base + rva as usize,
            base + original_rva as usize,
            replacement,
            opcode,
        ));
    }
    // SAFETY: exact executable ranges were checked above, and the caller holds the startup phase.
    unsafe { redirect_checked(&plan) }
}

unsafe fn redirect_checked(plan: &[(usize, usize, *const c_void, Opcode)]) -> Result<()> {
    let mut pages = Vec::new();
    for (i, &(from, original, _, opcode)) in plan.iter().enumerate() {
        let page = from & !(PAGE_LEN - 1);
        ensure!(
            from - page <= PAGE_LEN - CALL_LEN
                && !plan[..i]
                    .iter()
                    .any(|&(prior, _, _, _)| from.abs_diff(prior) < CALL_LEN),
            "title call crosses a page or overlaps another call at {from:#x}"
        );
        let expected =
            branch(from, original, opcode).context("original title branch is out of range")?;
        // SAFETY: caller verified that these five bytes are mapped code.
        ensure!(
            unsafe { (from as *const [u8; CALL_LEN]).read_unaligned() } == expected,
            "unexpected title call at {from:#x}"
        );
        if !pages.contains(&page) {
            pages.push(page);
        }
    }

    let memory = RelayMemory(
        crate::console_insn::near_with_protection(plan[0].0, PAGE_READWRITE)
            .context("allocating a title call relay within branch range")?,
    );
    let mut replacements = Vec::with_capacity(plan.len());
    for (i, &(from, _, target, opcode)) in plan.iter().enumerate() {
        let address = memory.0 + i * RELAY_LEN;
        let instruction =
            branch(from, address, opcode).context("title branch relay is out of range")?;
        let thunk = relay(target);
        // SAFETY: each relay gets its own slot in the new writable allocation.
        unsafe { std::ptr::copy_nonoverlapping(thunk.as_ptr(), address as *mut u8, RELAY_LEN) };
        replacements.push((from, instruction));
    }
    let size = plan.len() * RELAY_LEN;
    protect(memory.0, size, PAGE_EXECUTE_READ).context("sealing the title call relays")?;
    flush(memory.0, size).context("flushing the title call relays")?;
    let mut pages = pages
        .into_iter()
        .map(WritablePage::open)
        .collect::<Result<Vec<_>>>()?;

    // Keep these committed once a title call refers to them, also on an
    // unexpected post-write error (the caller then stops before running title code).
    std::mem::forget(memory);
    for (from, instruction) in replacements {
        // SAFETY: all involved pages are writable and no title thread can execute these bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(instruction.as_ptr().add(1), (from + 1) as *mut u8, 4);
        }
        flush(from, CALL_LEN).context("flushing a redirected title call")?;
    }
    for page in &mut pages {
        page.restore()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, VirtualAlloc};

    extern "system" fn replacement(value: u32) -> u32 {
        value + 7
    }

    #[test]
    fn startup_calls_validate_before_redirect_and_keep_win64_arguments() {
        // Reserve one page for a synthetic title with a normal Win64 call frame.
        let page = RelayMemory(unsafe {
            VirtualAlloc(
                std::ptr::null(),
                PAGE_LEN,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        } as usize);
        assert_ne!(page.0, 0);
        let from = page.0 + 4;
        let original = page.0 + 32;
        let mut code = [0xcc_u8; 35];
        code[..4].copy_from_slice(&[0x48, 0x83, 0xec, 0x28]);
        code[4..9].copy_from_slice(&branch(from, original, Opcode::Call).unwrap());
        code[9..14].copy_from_slice(&[0x48, 0x83, 0xc4, 0x28, 0xc3]);
        code[32..].copy_from_slice(&[0x89, 0xc8, 0xc3]); // mov eax,ecx; ret
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), page.0 as *mut u8, code.len()) };
        protect(page.0, PAGE_LEN, PAGE_EXECUTE_READ).unwrap();
        flush(page.0, code.len()).unwrap();
        // SAFETY: the fixture implements the exact call ABI and is now executable.
        let invoke: extern "system" fn(u32) -> u32 = unsafe { std::mem::transmute(page.0) };
        let target = replacement as *const c_void;
        assert_eq!(invoke(41), 41);
        assert!(
            unsafe {
                redirect_checked(&[
                    (from, original, target, Opcode::Call),
                    (page.0 + 14, original, target, Opcode::Call),
                ])
            }
            .is_err()
        );
        assert_eq!(invoke(41), 41);
        unsafe { redirect_checked(&[(from, original, target, Opcode::Call)]) }.unwrap();
        assert_eq!(invoke(41), 48);
    }

    #[test]
    fn startup_tail_jump_keeps_the_caller_frame_and_checks_its_opcode() {
        let page = RelayMemory(unsafe {
            VirtualAlloc(
                std::ptr::null(),
                PAGE_LEN,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        } as usize);
        assert_ne!(page.0, 0);
        let original = page.0 + 32;
        let mut code = [0xcc_u8; 35];
        code[..5].copy_from_slice(&branch(page.0, original, Opcode::TailJump).unwrap());
        code[32..].copy_from_slice(&[0x89, 0xc8, 0xc3]); // mov eax,ecx; ret
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), page.0 as *mut u8, code.len()) };
        protect(page.0, PAGE_LEN, PAGE_EXECUTE_READ).unwrap();
        flush(page.0, code.len()).unwrap();
        let invoke: extern "system" fn(u32) -> u32 = unsafe { std::mem::transmute(page.0) };
        let target = replacement as *const c_void;
        assert_eq!(invoke(41), 41);
        assert!(unsafe { redirect_checked(&[(page.0, original, target, Opcode::Call)]) }.is_err());
        assert_eq!(invoke(41), 41);
        unsafe { redirect_checked(&[(page.0, original, target, Opcode::TailJump)]) }.unwrap();
        assert_eq!(invoke(41), 48);
    }
}
