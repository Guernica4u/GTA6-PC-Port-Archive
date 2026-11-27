//! Direct digital reads of the title's aim and melee actions.

use std::ffi::c_void;

use anyhow::Result;
use victory_loader::LoadedImage;
use victory_loader::startup_calls::{StartupCall, StartupTailJump, redirect_with_tail_jumps};

use super::lockon::lockon_image_base;
use super::mouse_controls::{enabled, mouse_digital_pressed};

const NATIVE_DIGITAL_PRESS: u32 = 0x007c_cb90;
const AIM: usize = 0x034;
const MELEE_ATTACK: usize = 0x135;

unsafe fn merge_pressed(
    original: bool,
    action: *const u8,
    mouse_pressed: impl FnOnce(*const u8) -> bool,
) -> bool {
    if original {
        return true;
    }
    if action.is_null() || unsafe { action.add(0x158).read() } & 2 == 0 {
        return false;
    }

    // The native reader checks these same eligibility bits before returning
    // the digital press flag. A disabled or suppressed action stays disabled.
    let state = unsafe { action.add(0xd0) };
    let allowed = unsafe { state.add(0x45).read() == 1 && state.add(0x46).read() & 1 == 0 };
    allowed && mouse_pressed(state)
}

unsafe fn pressed(action: *const u8, id: usize) -> bool {
    let base = lockon_image_base();
    let original: unsafe extern "system" fn(*const u8) -> bool =
        unsafe { std::mem::transmute(base + NATIVE_DIGITAL_PRESS as usize) };
    let native_pressed = unsafe { original(action) };
    unsafe {
        merge_pressed(native_pressed, action, |state| {
            mouse_digital_pressed(base, state, id)
        })
    }
}

unsafe extern "system" fn read_aim(action: *const u8) -> bool {
    unsafe { pressed(action, AIM) }
}

unsafe extern "system" fn read_melee(action: *const u8) -> bool {
    unsafe { pressed(action, MELEE_ATTACK) }
}

/// # Safety
/// The loader has verified the exact title and installed the shared action
/// state before executing any title instructions.
pub(super) unsafe fn install(image: &LoadedImage) -> Result<()> {
    if !enabled() {
        return Ok(());
    }

    // At these calls the native resolver just loaded CPad AIM (0x2b20/22)
    // or MELEE_ATTACK (0x2f24/26). Other uses of the shared digital reader
    // keep their original entry points.
    let aim = [0x00cd_c8c9, 0x0204_0c03].map(|call_rva| StartupCall {
        call_rva,
        original_rva: NATIVE_DIGITAL_PRESS,
        replacement: read_aim as *const c_void,
    });
    let melee = [
        0x00bc_11a5,
        0x0159_56a5,
        0x02ea_e375,
        0x02fe_0f66,
        0x0305_1b7e,
        0x0305_1ecf,
        0x0430_d6a6,
        0x0475_6414,
    ]
    .map(|call_rva| StartupCall {
        call_rva,
        original_rva: NATIVE_DIGITAL_PRESS,
        replacement: read_melee as *const c_void,
    });
    let calls: Vec<_> = melee.into_iter().chain(aim).collect();
    let jumps = [StartupTailJump {
        jump_rva: 0x04ac_d9d5,
        original_rva: NATIVE_DIGITAL_PRESS,
        replacement: read_melee as *const c_void,
    }];

    unsafe { redirect_with_tail_jumps(image, &calls, &jumps) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_press_preserves_the_native_button_and_rejects_suppressed_actions() {
        let mut action = [0; 0x159];
        let absent = std::ptr::null();
        assert!(!unsafe { merge_pressed(false, absent, |_| panic!("absent action")) });

        action[0x158] = 2;
        action[0x115] = 1;
        assert!(!unsafe { merge_pressed(false, action.as_ptr(), |_| false) });
        assert!(unsafe {
            merge_pressed(false, action.as_ptr(), |state| {
                state == action.as_ptr().wrapping_add(0xd0)
            })
        });
        assert!(unsafe { merge_pressed(true, action.as_ptr(), |_| panic!("already pressed")) });
        action[0x116] = 1;
        assert!(!unsafe { merge_pressed(false, action.as_ptr(), |_| panic!("suppressed action")) });
        action[0x116] = 0;
        action[0x158] = 0;
        assert!(!unsafe { merge_pressed(false, action.as_ptr(), |_| panic!("inactive action")) });
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    #[ignore = "requires the verified local Scarlett executable"]
    fn original_digital_reader_observes_mouse_press_hold_and_release() {
        use std::io::{Read, Seek, SeekFrom};

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn VirtualAlloc(
                address: *mut c_void,
                len: usize,
                allocation: u32,
                protection: u32,
            ) -> *mut c_void;
            fn VirtualProtect(
                address: *mut c_void,
                len: usize,
                protection: u32,
                old: *mut u32,
            ) -> i32;
            fn VirtualFree(address: *mut c_void, len: usize, kind: u32) -> i32;
            fn GetCurrentProcess() -> *mut c_void;
            fn FlushInstructionCache(
                process: *mut c_void,
                address: *const c_void,
                len: usize,
            ) -> i32;
        }

        let path = std::env::var_os("VICTORY_TEST_NATIVE_ACTION_PE").expect("path to title PE");
        let mut title = std::fs::File::open(path).unwrap();
        let mut code = [0xcc; 4096];
        let fragments: [(u64, usize, &[u8]); 3] = [
            (
                0x7ccb90,
                0,
                &[
                    0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x85, 0xc9, 0x74, 0x28, 0x8a, 0x99, 0x58,
                    0x01, 0x00, 0x00, 0xf6, 0xc3, 0x02, 0x74, 0x1d, 0xe8, 0xc6, 0xf0, 0x84, 0x04,
                    0x80, 0x78, 0x45, 0x01, 0x75, 0x12, 0xf6, 0x40, 0x46, 0x01, 0x75, 0x0c, 0x89,
                    0xd9, 0x48, 0x83, 0xc4, 0x20, 0x5b, 0xe9, 0xae, 0x8d, 0x83, 0xff, 0x31, 0xc0,
                    0x48, 0x83, 0xc4, 0x20, 0x5b, 0xc3,
                ],
            ),
            (0x501bc70, 0x100, &[0x48, 0x8d, 0x81, 0xd0, 0, 0, 0, 0xc3]),
            (0x5970, 0x120, &[0x89, 0xc8, 0x24, 0x01, 0xc3]),
        ];
        for (rva, offset, expected) in fragments {
            title.seek(SeekFrom::Start(rva - 0xc00)).unwrap();
            let bytes = &mut code[offset..offset + expected.len()];
            title.read_exact(bytes).unwrap();
            assert_eq!(bytes, expected);
        }
        // Only the two relative targets move. The instruction bodies and
        // Win64 argument/return conventions match the actual title.
        code[0x16..0x1a].copy_from_slice(&(0x100_i32 - 0x1a).to_le_bytes());
        code[0x2e..0x32].copy_from_slice(&(0x120_i32 - 0x32).to_le_bytes());
        let memory = unsafe { VirtualAlloc(std::ptr::null_mut(), code.len(), 0x3000, 4) };
        assert!(!memory.is_null());
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), memory.cast::<u8>(), code.len()) };
        let mut old = 0;
        assert_ne!(
            unsafe { VirtualProtect(memory, code.len(), 0x20, &mut old) },
            0
        );
        assert_ne!(
            unsafe { FlushInstructionCache(GetCurrentProcess(), memory, code.len()) },
            0
        );
        let reader: unsafe extern "system" fn(*const u8) -> bool =
            unsafe { std::mem::transmute(memory) };

        for flags in 0..16u8 {
            for allowed in 0..3u8 {
                for suppressed in 0..4u8 {
                    let mut action = [0; 0x159];
                    let row = [
                        0,
                        1.0_f32.to_bits(),
                        0,
                        1.0_f32.to_bits(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ];
                    action[0xd4..0xd8].copy_from_slice(&1u32.to_le_bytes());
                    action[0xd8..0xe0].copy_from_slice(&(row.as_ptr() as usize).to_le_bytes());
                    action[0x158] = flags;
                    action[0x115] = allowed;
                    action[0x116] = suppressed;
                    let before = unsafe { reader(action.as_ptr()) };

                    action[0x158] = flags | 1;
                    let physical_press = unsafe { reader(action.as_ptr()) };
                    action[0x158] = flags;
                    let after = [false, true, true, false].map(|mouse_pressed| unsafe {
                        merge_pressed(before, action.as_ptr(), |_| mouse_pressed)
                    });
                    assert_eq!(after, [before, physical_press, physical_press, before]);
                    assert_eq!(action[0x158], flags);
                    assert_eq!(action[0x115], allowed);
                    assert_eq!(action[0x116], suppressed);
                }
            }
        }
        assert_ne!(unsafe { VirtualFree(memory, 0, 0x8000) }, 0);
    }
}
