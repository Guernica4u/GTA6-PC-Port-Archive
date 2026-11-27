//! Keyboard glyph substitution for the title's on-foot lock-on prompt.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

const NATIVE_PARSER: usize = 0x05b6_c900;
const LOCKON: &[u8] = b"INPUT_INTERACT_LOCKON";
const KIND_OFFSET: usize = 0x1c;
const COUNT_OFFSET: usize = 0x18;
const KIND_INPUT: u32 = 5;
const LT_ORDINAL: u32 = 35;
const F_ORDINAL: u32 = 33; // Y in the title's original controller glyph sheet.

unsafe fn is_lockon(mut token: *const u8) -> bool {
    if token.is_null() {
        return false;
    }
    // SAFETY: the native parser is called with a NUL-terminated string. Stop
    // at the first mismatch, so a shorter token is never read past its NUL.
    unsafe {
        if token.read() == b'~' {
            token = token.add(1);
        }
        for (position, &expected) in LOCKON.iter().enumerate() {
            if token.add(position).read() != expected {
                return false;
            }
        }
        matches!(token.add(LOCKON.len()).read(), 0 | b'~' | b'|')
    }
}

pub(super) unsafe extern "system" fn parse_lockon_prompt(
    context: *mut c_void,
    token: *const u8,
    output: *mut u8,
) -> bool {
    // SAFETY: the exact renderer call and its original parser are verified
    // before the callback can run. Forward all parameters unchanged.
    let native: unsafe extern "system" fn(*mut c_void, *const u8, *mut u8) -> bool =
        unsafe { std::mem::transmute(super::lockon_image_base() + NATIVE_PARSER) };
    let ok = unsafe { native(context, token, output) };
    if !ok || output.is_null() || !super::keyboard_lockon_active() {
        return ok;
    }

    // Only the icon selection in the completed input descriptor changes.
    let exact = unsafe { is_lockon(token) };
    let replacement = unsafe { remap(output, exact) };
    if super::trace_lockon() {
        if exact {
            static MATCHES: AtomicUsize = AtomicUsize::new(0);
            if MATCHES.fetch_add(1, Ordering::Relaxed) < 8 {
                eprintln!("VIctory F lock-on glyph: exact token, remapped={replacement}");
            }
        } else if !token.is_null() && unsafe { has_physical_prefix(token) } {
            static PHYSICAL: AtomicUsize = AtomicUsize::new(0);
            if PHYSICAL.fetch_add(1, Ordering::Relaxed) < 4 {
                eprintln!("VIctory F lock-on glyph: native physical token observed");
            }
        }
    }
    ok
}

unsafe fn remap(output: *mut u8, exact: bool) -> bool {
    if !exact {
        return false;
    }
    // SAFETY: the parser just produced this glyph descriptor.
    unsafe {
        if output.add(KIND_OFFSET).cast::<u32>().read_unaligned() != KIND_INPUT
            || output.add(COUNT_OFFSET).cast::<u32>().read_unaligned() != 1
            || output.cast::<u32>().read_unaligned() != LT_ORDINAL
        {
            return false;
        }
        output.cast::<u32>().write_unaligned(F_ORDINAL);
    }
    true
}

unsafe fn has_physical_prefix(mut token: *const u8) -> bool {
    // SAFETY: same parser input as is_lockon. 'all' stops on the first
    // mismatch, including a NUL earlier than the eight-byte prefix.
    unsafe {
        if token.read() == b'~' {
            token = token.add(1);
        }
        b"INPUTHC_"
            .iter()
            .enumerate()
            .all(|(position, &byte)| token.add(position).read() == byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_prompt_changes_only_the_single_left_trigger_glyph() {
        for token in [
            b"INPUT_INTERACT_LOCKON\0".as_slice(),
            b"~INPUT_INTERACT_LOCKON~\0",
            b"INPUT_INTERACT_LOCKON|INPUT_AIM\0",
        ] {
            assert!(unsafe { is_lockon(token.as_ptr()) });
        }
        for token in [
            b"\0".as_slice(),
            b"INPUT_INTERACT_LOCKON_A\0",
            b"INPUT_INTERACT_LOCKON_VEH\0",
            b"INPUT_AIM\0",
            b"INPUTHC_0xDEADBEEF\0",
        ] {
            assert!(!unsafe { is_lockon(token.as_ptr()) });
        }

        let mut glyph = [LT_ORDINAL, 1, 2, 3, 4, 5, 1, KIND_INPUT];
        let output = glyph.as_mut_ptr().cast::<u8>();
        assert!(!unsafe { remap(output, false) });
        assert_eq!(glyph[0], LT_ORDINAL);
        assert!(unsafe { remap(output, true) });
        assert_eq!(glyph, [F_ORDINAL, 1, 2, 3, 4, 5, 1, KIND_INPUT]);

        let mut compound = [LT_ORDINAL, 0, 0, 0, 0, 0, 2, KIND_INPUT];
        assert!(!unsafe { remap(compound.as_mut_ptr().cast::<u8>(), true) });
        assert_eq!(compound[0], LT_ORDINAL);
    }
}
