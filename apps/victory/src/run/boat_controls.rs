//! Keyboard throttle and brake for the verified Scarlett 1.0.2.1 boat driver.

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};

use anyhow::{Result, ensure};
use victory_core::input::pad_buttons;
use victory_core::input::profile::{self, Action as InputAction};
use victory_hle::modules::gameinput::{self, BoatKeyboardInput};
use victory_loader::LoadedImage;
use victory_loader::startup_calls::{StartupCall, redirect};

const TITLE_HASH: &str = "b99f876b74d4a77f03af5f878223b27bd647b5c35a79d5decd9f716504239e93";
const LOCAL_PLAYER: usize = 0x0ab5_6e38;
const PAD_ONE: usize = 0x0ab7_78d0;
const VEHICLE_OF_PED: usize = 0x04dd_7510;
const DRIVER_OF_VEHICLE: usize = 0x04eb_3750;
const GAMEPLAY_PAD_OF_PED: usize = 0x0507_18b0;
const WRITE_TRIGGER: u32 = 0x0439_eef0;
const WRITE_BUTTON: u32 = 0x0439_eeb0;
const WRITE_LEFT_STICK: u32 = 0x001f_ec90;

static IMAGE_BASE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Context {
    Idle,
    NoBoat,
    Passenger,
    GameplayInputBlocked,
    BoatDriver,
}

impl Context {
    fn from_seat(is_boat: bool, in_driver_seat: bool, allows_gameplay: bool) -> Self {
        if !is_boat {
            Self::NoBoat
        } else if !in_driver_seat {
            Self::Passenger
        } else if !allows_gameplay {
            Self::GameplayInputBlocked
        } else {
            Self::BoatDriver
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    device: *mut u8,
    context: Context,
    kind: u8,
    keys: Option<BoatKeyboardInput>,
}

impl Sample {
    fn driving_keys(self) -> Option<BoatKeyboardInput> {
        self.keys.filter(|_| self.context == Context::BoatDriver)
    }

    fn brakes(self) -> bool {
        self.driving_keys().is_some_and(|keys| keys.brake)
    }

    fn accelerates(self) -> bool {
        self.driving_keys()
            .is_some_and(|keys| keys.accelerate && !keys.brake)
    }
}

thread_local! {
    // Both triggers and the left stick are written during one title input update.
    static SAMPLE: Cell<Option<Sample>> = const { Cell::new(None) };
}

fn is_boat(kind: u8) -> bool {
    // The original steering selector at 0x2d4a29 picks INPUT_VEH_BOAT_TURN_LR
    // (0x112) for these three kinds, as recorded in the title enum table.
    (9..=11).contains(&kind)
}

unsafe fn gameplay_context(base: usize) -> (Context, u8) {
    // These reads visit only the active title's player. The current-player
    // pointer and entity ID layout are the ones used by the native action
    // selector at 0x501be59. All routines run on the title input update thread.
    let player = unsafe { ((base + LOCAL_PLAYER) as *const *const u8).read() };
    if player.is_null() {
        return (Context::NoBoat, 0);
    }
    let vehicle_of: unsafe extern "system" fn(u32) -> *const u8 =
        unsafe { std::mem::transmute(base + VEHICLE_OF_PED) };
    let driver_of: unsafe extern "system" fn(u32) -> *const u8 =
        unsafe { std::mem::transmute(base + DRIVER_OF_VEHICLE) };
    let gameplay_pad: unsafe extern "system" fn(*const u8, u8) -> *const u8 =
        unsafe { std::mem::transmute(base + GAMEPLAY_PAD_OF_PED) };
    let vehicle = unsafe { vehicle_of(player.add(0x9c).cast::<u32>().read_unaligned()) };
    if vehicle.is_null() {
        return (Context::NoBoat, 0);
    }
    let kind = unsafe { vehicle.add(0xfa).read() };
    let boat = is_boat(kind);
    // The native getter resolves the vehicle's configured driver seat and
    // verifies its occupant handle before returning the ped pointer.
    let in_driver_seat =
        boat && unsafe { driver_of(vehicle.add(0x9c).cast::<u32>().read_unaligned()) } == player;
    let allows_gameplay =
        in_driver_seat && unsafe { gameplay_pad(player, 1) } as usize == base + PAD_ONE;
    (
        Context::from_seat(boat, in_driver_seat, allows_gameplay),
        kind,
    )
}

fn digital_trigger(original: f32, pressed: bool) -> f32 {
    if pressed { 1.0 } else { original }
}

fn trace_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_BOAT_CONTROLS_TRACE").is_some_and(|v| v == "1"))
}

unsafe fn trace(sample: Sample) {
    if !trace_enabled() {
        return;
    }
    let Sample { kind, context, .. } = sample;
    let keys = sample.keys.unwrap_or_default();
    let value = (u16::from(kind) << 8)
        | ((context as u16) << 4)
        | (u16::from(keys.accelerate) << 1)
        | u16::from(keys.brake);
    static LAST: AtomicU16 = AtomicU16::new(u16::MAX);
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    if LAST.swap(value, Ordering::Relaxed) != value && COUNT.fetch_add(1, Ordering::Relaxed) < 48 {
        // The original trigger writer just stored LT and RT at these offsets.
        let brake = unsafe { sample.device.add(0x48).cast::<f32>().read_unaligned() };
        let throttle = unsafe { sample.device.add(0x4c).cast::<f32>().read_unaligned() };
        eprintln!(
            "VIctory boat input: context={context:?} kind={kind:#x} w={} s={} throttle={throttle:.3} brake={brake:.3}",
            u8::from(keys.accelerate),
            u8::from(keys.brake),
        );
    }
}

unsafe extern "system" fn write_left_trigger(
    device: *mut u8,
    axis: u32,
    raw: f32,
    deadzone: f32,
    limit: f32,
) {
    let base = IMAGE_BASE.load(Ordering::Acquire);
    let keys = gameinput::boat_keyboard_input().filter(|keys| keys.accelerate || keys.brake);
    // The exact first trigger call is in the title's GameInput update,
    // after it sampled the merged pad. Install only in one-pad mode.
    let (context, kind) = keys.map_or((Context::Idle, 0), |_| unsafe { gameplay_context(base) });
    let sample = Sample {
        device,
        context,
        kind,
        keys,
    };
    SAMPLE.set(Some(sample));
    // Preserve the calibrated trigger path and the other native parameters.
    let write: unsafe extern "system" fn(*mut u8, u32, f32, f32, f32) =
        unsafe { std::mem::transmute(base + WRITE_TRIGGER as usize) };
    unsafe {
        write(
            device,
            axis,
            digital_trigger(raw, sample.brakes()),
            deadzone,
            limit,
        )
    };
}

unsafe extern "system" fn write_right_trigger(
    device: *mut u8,
    axis: u32,
    raw: f32,
    deadzone: f32,
    limit: f32,
) {
    let sample = SAMPLE.get().filter(|sample| sample.device == device);
    let requested = sample.is_some_and(Sample::accelerates);
    let base = IMAGE_BASE.load(Ordering::Acquire);
    let write: unsafe extern "system" fn(*mut u8, u32, f32, f32, f32) =
        unsafe { std::mem::transmute(base + WRITE_TRIGGER as usize) };
    unsafe {
        write(
            device,
            axis,
            digital_trigger(raw, requested),
            deadzone,
            limit,
        )
    };
    if let Some(sample) = sample {
        unsafe { trace(sample) };
    }
}

unsafe extern "system" fn write_left_stick(device: *mut u8, x: f32, y: f32) {
    let keys = SAMPLE
        .take()
        .filter(|sample| sample.device == device)
        .and_then(Sample::driving_keys);
    // Remove just W/S from this axis. The title inverts the Y sign at
    // 0x7a44f6 before calling the original stick setter.
    let y = keys.map_or(y, |keys| -keys.physical_left_y);
    let base = IMAGE_BASE.load(Ordering::Acquire);
    let write: unsafe extern "system" fn(*mut u8, f32, f32) =
        unsafe { std::mem::transmute(base + WRITE_LEFT_STICK as usize) };
    unsafe { write(device, x, y) };
}

fn handbrake_button(raw: u32, context: Context, space: bool) -> u32 {
    if space && context == Context::BoatDriver {
        raw | pad_buttons::A
    } else {
        raw
    }
}

// The verified 0x7a438c call writes A (PAD_RDOWN), the standard layout's
// native VEHICLE_HANDBRAKE as taught by the original speedboat mission.
unsafe extern "system" fn write_handbrake(device: *mut u8, button: u32, raw: u32) {
    let base = IMAGE_BASE.load(Ordering::Acquire);
    let space = gameinput::boat_keyboard_input().is_some_and(|keys| keys.handbrake);
    let (context, kind) = if space {
        unsafe { gameplay_context(base) }
    } else {
        (Context::Idle, 0)
    };
    let input = handbrake_button(raw, context, space);
    let write: unsafe extern "system" fn(*mut u8, u32, u32) =
        unsafe { std::mem::transmute(base + WRITE_BUTTON as usize) };
    unsafe { write(device, button, input) };
    if trace_enabled() {
        static LAST: AtomicU16 = AtomicU16::new(u16::MAX);
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let value = (u16::from(kind) << 8) | ((context as u16) << 4) | u16::from(space);
        if LAST.swap(value, Ordering::Relaxed) != value
            && COUNT.fetch_add(1, Ordering::Relaxed) < 24
        {
            // The title's own writer stores the button's analog value here.
            let native_a = unsafe { device.add(0x48 + 4 * 6).cast::<f32>().read_unaligned() };
            eprintln!(
                "VIctory boat handbrake: context={context:?} kind={kind:#x} space={} native_a={native_a:.3}",
                u8::from(space),
            );
        }
    }
}

/// Installs the checked input calls before the supported title runs.
///
/// # Safety
/// The main game image is still mapped and has not started executing.
pub(super) unsafe fn install(image: &LoadedImage, digest: &str) -> Result<()> {
    if !std::env::var_os("VICTORY_BOAT_CONTROLS").is_some_and(|value| value == "1") {
        return Ok(());
    }
    ensure!(
        cfg!(target_arch = "x86_64") && digest == TITLE_HASH,
        "boat controls require the verified x64 1.0.2.1 title image"
    );
    ensure!(
        std::env::var_os("VICTORY_KEYBOARD_PAD").is_none_or(|v| v != "0")
            && std::env::var_os("VICTORY_ONE_PAD").is_none_or(|v| v != "0"),
        "boat controls require the merged keyboard gamepad"
    );
    let mut calls = vec![
        StartupCall {
            call_rva: 0x007a_4494,
            original_rva: WRITE_TRIGGER,
            replacement: write_left_trigger as *const c_void,
        },
        StartupCall {
            call_rva: 0x007a_44dd,
            original_rva: WRITE_TRIGGER,
            replacement: write_right_trigger as *const c_void,
        },
        StartupCall {
            call_rva: 0x007a_4671,
            original_rva: WRITE_LEFT_STICK,
            replacement: write_left_stick as *const c_void,
        },
    ];
    let handbrake = std::env::var_os("VICTORY_BOAT_HANDBRAKE").is_some_and(|value| value == "1");
    if handbrake {
        calls.push(StartupCall {
            call_rva: 0x007a_438c,
            original_rva: WRITE_BUTTON,
            replacement: write_handbrake as *const c_void,
        });
    }
    // SAFETY: the exact image hash and the original calls are checked before
    // the title starts. Each replacement keeps the original Win64 arguments.
    unsafe { redirect(image, &calls) }?;
    IMAGE_BASE.store(image.base(), Ordering::Release);
    gameinput::enable_boat_keyboard_input();
    let keys = profile::current();
    println!("keyboard       {}/{} control throttle and brake in the boat driver's seat",
        keys.binding(InputAction::BoatAccelerate), keys.binding(InputAction::BoatBrake));
    if handbrake {
        println!("keyboard       {} uses the boat's native handbrake", keys.binding(InputAction::BoatHandbrake));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boat_controls_use_only_native_boat_vehicle_kinds() {
        for kind in [0, 3, 8, 12, u8::MAX] {
            assert!(!is_boat(kind));
        }
        for kind in 9..=11 {
            assert!(is_boat(kind));
        }
    }

    #[test]
    fn boat_controls_require_the_native_driver_seat_and_gameplay_pad() {
        for is_boat in [false, true] {
            for in_driver_seat in [false, true] {
                for allows_gameplay in [false, true] {
                    assert_eq!(
                        Context::from_seat(is_boat, in_driver_seat, allows_gameplay)
                            == Context::BoatDriver,
                        is_boat && in_driver_seat && allows_gameplay
                    );
                }
            }
        }
    }

    #[test]
    fn simultaneous_keyboard_throttle_and_brake_apply_only_the_brake() {
        let sample = Sample {
            device: std::ptr::null_mut(),
            context: Context::BoatDriver,
            kind: 10,
            keys: Some(BoatKeyboardInput {
                accelerate: true,
                brake: true,
                handbrake: false,
                physical_left_y: 0.35,
            }),
        };
        assert!(!sample.accelerates());
        assert!(sample.brakes());
        assert_eq!(sample.driving_keys().unwrap().physical_left_y, 0.35);

        let sample = Sample {
            context: Context::Passenger,
            ..sample
        };
        assert!(!sample.accelerates());
        assert!(!sample.brakes());
        assert!(sample.driving_keys().is_none());
    }

    #[test]
    fn boat_handbrake_adds_a_only_for_space_in_the_driving_seat() {
        for context in [
            Context::Idle,
            Context::NoBoat,
            Context::Passenger,
            Context::GameplayInputBlocked,
        ] {
            assert_eq!(handbrake_button(0, context, true), 0);
            assert_eq!(
                handbrake_button(pad_buttons::A, context, true),
                pad_buttons::A
            );
        }
        assert_eq!(
            handbrake_button(0, Context::BoatDriver, true),
            pad_buttons::A
        );
        assert_eq!(handbrake_button(0, Context::BoatDriver, false), 0);
        assert_eq!(
            handbrake_button(pad_buttons::A, Context::BoatDriver, false),
            pad_buttons::A
        );
    }

    #[test]
    fn boat_controls_apply_full_trigger_only_for_a_requested_keyboard_axis() {
        assert_eq!(digital_trigger(0.45, false), 0.45);
        assert_eq!(digital_trigger(0.45, true), 1.0);
        assert_eq!(digital_trigger(0.0, false), 0.0);
    }
}
