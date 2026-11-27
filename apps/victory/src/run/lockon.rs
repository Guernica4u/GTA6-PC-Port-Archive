//! Scoped keyboard actions and the shared action-row reads for Scarlett 1.0.2.1.

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::{Result, ensure};
use victory_core::input::profile::{self, Action as InputAction};
use victory_core::input::{ActiveInput, active_input, pad_buttons};
use victory_loader::LoadedImage;
use victory_loader::startup_calls::{StartupCall, redirect};

const TITLE_IMAGE: &str = "b99f876b74d4a77f03af5f878223b27bd647b5c35a79d5decd9f716504239e93";
const PARSE_GLYPH: u32 = 0x05b6_c900;
const PAD_BUTTON_WRITE: u32 = 0x0439_eeb0;
const ACTION_ROW_READ: u32 = 0x04db_8680;
const PAD_ONE: usize = 0x0ab7_78d0;
const RESOLVE_ACTION: usize = 0x0501_bdd0;
const INTERACT_LOCKON: usize = 0x210;
const PICKUP: usize = 0x04d;
const LOCAL_PLAYER: usize = 0x0ab5_6e38;
const VEHICLE_OF_PED: usize = 0x04dd_7510;
const GAMEPLAY_PAD_OF_PED: usize = 0x0507_18b0;

// These are native actions with their own input purpose. If a binding scheme
// makes one share the pickup action object, keep that scheme's original input.
const PICKUP_EXCLUSIONS: [usize; 7] = [
    0x032, // INPUT_ENTER
    0x0a9, // INPUT_VEH_EXIT
    0x18c, // INPUT_FRONTEND_ACCEPT
    0x190, // INPUT_FRONTEND_LB, also used by the original weapon pickup prompt
    INTERACT_LOCKON,
    0x257, // INPUT_LOOT
    0x25d, // INPUT_SHOP_BUY
];

const USE_LOCKON: u8 = 1;
const USE_PICKUP: u8 = 2;
const CURRENT: u8 = 1;
const PREVIOUS: u8 = 2;

type ActionRow = [u32; 12];

static BASE: AtomicUsize = AtomicUsize::new(0);
static ACTIONS: AtomicU8 = AtomicU8::new(0);
static SAMPLES: AtomicU8 = AtomicU8::new(0);
static PAD_SAMPLE: Mutex<PadSample> = Mutex::new(PadSample { device: 0, bits: 0 });

thread_local! {
    // The two title callers consume the returned row immediately.
    static ROW: Cell<ActionRow> = const { Cell::new([0; 12]) };
}

pub(super) fn lockon_image_base() -> usize {
    BASE.load(Ordering::Acquire)
}

pub(super) fn keyboard_lockon_active() -> bool {
    active_input() == ActiveInput::Keyboard
}

pub(super) fn trace_lockon() -> bool {
    static TRACE: OnceLock<bool> = OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("VICTORY_F_LOCKON_TRACE").is_some_and(|v| v == "1"))
}

fn trace_pickup() -> bool {
    static TRACE: OnceLock<bool> = OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("VICTORY_F_PICKUP_TRACE").is_some_and(|v| v == "1"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    LockOn,
    Pickup,
}

fn action_for_state(
    enabled: u8,
    mut is_state: impl FnMut(usize) -> bool,
    pickup_allowed: impl FnOnce() -> bool,
) -> Option<Action> {
    if enabled & USE_LOCKON != 0 && is_state(INTERACT_LOCKON) {
        return Some(Action::LockOn);
    }

    if enabled & USE_PICKUP != 0
        && is_state(PICKUP)
        && !PICKUP_EXCLUSIONS.into_iter().any(is_state)
        && pickup_allowed()
    {
        return Some(Action::Pickup);
    }

    None
}

fn active_keyboard_samples(input: ActiveInput, samples: u8) -> u8 {
    if input == ActiveInput::Keyboard {
        samples
    } else {
        0
    }
}

struct PadSample {
    device: usize,
    bits: u8,
}

impl PadSample {
    fn advance(&mut self, device: usize, pressed: [bool; 2]) -> u8 {
        // Only the title device receiving the keyboard sample advances its
        // history. A fresh press may claim an idle sample source.
        let current = u8::from(pressed[0]) | (u8::from(pressed[1]) << 2);
        if self.device == device || (current != 0 && self.bits == 0) {
            self.device = device;
            self.bits = (self.bits & 0b0101) << 1 | current;
        }
        self.bits
    }
}

// This precise call writes the raw Y button once per title pad update. The
// device has already aggregated its GameInput readings. Keep the pad write
// and publish a separate sample of the keyboard's contribution to Y.
unsafe extern "system" fn observe_y(device: *mut u8, button: u32, pressed: u32) {
    let base = lockon_image_base();
    // SAFETY: the exact title and call destination are checked at startup.
    let native: unsafe extern "system" fn(*mut u8, u32, u32) =
        unsafe { std::mem::transmute(base + PAD_BUTTON_WRITE as usize) };
    // SAFETY: the original receiver and arguments are passed to their writer.
    unsafe { native(device, button, pressed) };
    let keys = if button == 4 && keyboard_lockon_active() {
        let [interact, pickup] = victory_hle::modules::gameinput::interaction_keyboard_buttons();
        [interact && pressed & pad_buttons::Y != 0, pickup]
    } else {
        [false; 2]
    };
    let mut sample = PAD_SAMPLE.lock().unwrap_or_else(|err| err.into_inner());
    let bits = sample.advance(device as usize, keys);
    let before = SAMPLES.swap(bits, Ordering::AcqRel);
    if before != bits && ACTIONS.load(Ordering::Acquire) & USE_LOCKON != 0 && trace_lockon() {
        static LOGGED: AtomicUsize = AtomicUsize::new(0);
        if LOGGED.fetch_add(1, Ordering::Relaxed) < 24 {
            eprintln!(
                "VIctory F lock-on native Y update: current={} previous={}",
                bits & CURRENT != 0,
                bits & PREVIOUS != 0,
            );
        }
    }
}

pub(super) unsafe fn active_action_state(base: usize, action_id: usize) -> Option<*const u8> {
    let pad = (base + PAD_ONE) as *const u8;
    let binding_offset = 0x2a50 + 4 * action_id;
    // SAFETY: these fields of the title's first CPad match the original
    // lookup at 0x501bd50 and the two on-foot task callers.
    let (actions, scheme, index, binding_scheme) = unsafe {
        (
            pad.add(0x308).cast::<*const u8>().read_unaligned(),
            pad.add(0x2a45).read(),
            pad.add(binding_offset).cast::<u16>().read_unaligned(),
            pad.add(binding_offset + 2).read(),
        )
    };
    if actions.is_null() || index == 0 || binding_scheme == 0 {
        return None;
    }
    let resolve: unsafe extern "system" fn(*const u8, u8, u16, u8) -> *const u8 =
        // SAFETY: verified native action resolver in this fixed title build.
        unsafe { std::mem::transmute(base + RESOLVE_ACTION) };
    // SAFETY: the same fields the native selector forwards to this resolver.
    let action = unsafe { resolve(actions, scheme, index, binding_scheme) };
    // SAFETY: the title checks the same active action bit.
    (!action.is_null() && unsafe { action.add(0x158).read() } & 2 != 0)
        .then(|| action.wrapping_add(0xd0))
}

unsafe fn is_action_state(base: usize, state: *const u8, action_id: usize) -> bool {
    (unsafe { active_action_state(base, action_id) }) == Some(state)
}

/// The title's pause and other blocking interfaces withhold the gameplay pad.
/// The second result is the on-foot player, for the game's combat actions.
pub(super) unsafe fn mouse_gameplay_context(base: usize) -> (bool, Option<*const u8>) {
    // Read the local character the original action selector uses (0x501be59).
    let player = unsafe { ((base + LOCAL_PLAYER) as *const *const u8).read() };
    if player.is_null() {
        return (false, None);
    }
    let gameplay_pad: unsafe extern "system" fn(*const u8, u8) -> *const u8 =
        unsafe { std::mem::transmute(base + GAMEPLAY_PAD_OF_PED) };
    // The native pickup caller at 0x4708dfa also asks this title routine for
    // the gameplay pad before resolving INPUT_PICKUP. The same pad gate was
    // observed to close in the pause menu while the boat driver stays seated.
    if unsafe { gameplay_pad(player, 1) } as usize != base + PAD_ONE {
        return (true, None);
    }
    let vehicle_of: unsafe extern "system" fn(u32) -> *const u8 =
        unsafe { std::mem::transmute(base + VEHICLE_OF_PED) };
    let player_id = unsafe { player.add(0x9c).cast::<u32>().read_unaligned() };
    (false, unsafe { vehicle_of(player_id) }.is_null().then_some(player))
}

pub(super) unsafe fn on_foot_gameplay_player(base: usize) -> Option<*const u8> {
    unsafe { mouse_gameplay_context(base) }.1
}

fn with_keyboard(mut row: ActionRow, bits: u8) -> ActionRow {
    if bits & CURRENT != 0 {
        row[1] = 1.0_f32.to_bits();
    }
    if bits & PREVIOUS != 0 {
        row[3] = 1.0_f32.to_bits();
    }
    row
}

// The caller keeps the action state in nonvolatile RDI. Put that pointer in
// the unused fourth argument register, with the original stack and ABI.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
unsafe extern "system" fn select_row(_: u32, _: *const ActionRow, _: u32) -> *const ActionRow {
    std::arch::naked_asm!("mov r9, rdi", "jmp {}", sym select_row_for_state);
}

unsafe extern "system" fn select_row_for_state(
    count: u32,
    rows: *const ActionRow,
    index: u32,
    state: *const u8,
) -> *const ActionRow {
    let base = lockon_image_base();
    let native: unsafe extern "system" fn(u32, *const ActionRow, u32) -> *const ActionRow =
        // SAFETY: verified as the target of both native getters at startup.
        unsafe { std::mem::transmute(base + ACTION_ROW_READ as usize) };
    // SAFETY: arguments from the original call, including any empty row table.
    let original = unsafe { native(count, rows, index) };
    if index != 0 || count == 0 || original.is_null() {
        return original;
    }
    // SAFETY: this is the current native state and its valid first row.
    unsafe { super::weapon_wheel::observe_native_row(state, original) };
    let keyboard = active_keyboard_samples(active_input(), SAMPLES.load(Ordering::Acquire));
    let action = if keyboard == 0 {
        None
    } else {
        action_for_state(
            ACTIONS.load(Ordering::Acquire),
            // SAFETY: the detour supplied the live state at these exact callsites.
            |id| unsafe { is_action_state(base, state, id) },
            || unsafe { on_foot_gameplay_player(base) }.is_some(),
        )
    };
    // SAFETY: both routes match the live action object from the same first pad.
    let mouse = unsafe { super::mouse_controls::for_action_row(base, state) };
    let keyboard = match action {
        Some(Action::LockOn) => keyboard & 3,
        Some(Action::Pickup) => (keyboard >> 2) & 3,
        None => 0,
    };
    let bits = keyboard | mouse;
    if bits == 0 {
        return original;
    }

    // SAFETY: a valid index in the native table names a complete 48-byte row.
    let replacement = with_keyboard(unsafe { original.read_unaligned() }, bits);
    if let Some(action) = action.filter(|_| keyboard != 0) {
        trace_action(action, keyboard, &replacement);
    }
    ROW.with(|row| {
        row.set(replacement);
        row.as_ptr().cast_const()
    })
}

fn trace_action(action: Action, bits: u8, row: &ActionRow) {
    let (enabled, slot, label) = match action {
        Action::LockOn => (trace_lockon(), 0, "lock-on action 0x210"),
        Action::Pickup => (trace_pickup(), 1, "pickup action 0x04d"),
    };
    if !enabled {
        return;
    }
    static LAST: [AtomicU8; 2] = [const { AtomicU8::new(0) }; 2];
    static LOGGED: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
    if LAST[slot].swap(bits, Ordering::Relaxed) != bits
        && LOGGED[slot].fetch_add(1, Ordering::Relaxed) < 24
    {
        eprintln!(
            "VIctory F {label}: current={} previous={}",
            f32::from_bits(row[1]),
            f32::from_bits(row[3]),
        );
    }
}

/// Install keyboard sampling and the matching native actions before execution.
///
/// # Safety
/// This exact main title remains mapped, and none of its code has run yet.
pub(super) unsafe fn install(image: &LoadedImage, digest: &str) -> Result<()> {
    let lockon = std::env::var_os("VICTORY_F_LOCKON").is_some_and(|v| v == "1");
    let pickup = std::env::var_os("VICTORY_F_PICKUP").is_some_and(|v| v == "1");
    if !lockon && !pickup && !super::mouse_controls::enabled() {
        return Ok(());
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (image, digest);
        anyhow::bail!("native keyboard and mouse actions require the x64 host");
    }
    #[cfg(target_arch = "x86_64")]
    {
        ensure!(
            digest == TITLE_IMAGE,
            "native keyboard and mouse actions require the verified 1.0.2.1 title image"
        );
        ensure!(
            std::env::var_os("VICTORY_KEYBOARD_PAD").is_none_or(|v| v != "0")
                && std::env::var_os("VICTORY_ONE_PAD").is_none_or(|v| v != "0"),
            "native keyboard and mouse actions require the merged keyboard gamepad"
        );
        let mut sites = vec![
            StartupCall {
                call_rva: 0x04db_8656,
                original_rva: ACTION_ROW_READ,
                replacement: select_row as *const c_void,
            },
            StartupCall {
                call_rva: 0x0501_bd26,
                original_rva: ACTION_ROW_READ,
                replacement: select_row as *const c_void,
            },
        ];
        if lockon || pickup {
            sites.push(StartupCall {
                call_rva: 0x007a_4364,
                original_rva: PAD_BUTTON_WRITE,
                replacement: observe_y as *const c_void,
            });
        }
        if lockon {
            sites.push(StartupCall {
                call_rva: 0x01be_3e06,
                original_rva: PARSE_GLYPH,
                replacement: super::lockon_prompt::parse_lockon_prompt as *const c_void,
            });
        }
        // SAFETY: the mapped build and original CALLs are checked before
        // editing, and the title is stopped before its first instruction.
        unsafe { redirect(image, &sites) }?;
        BASE.store(image.base(), Ordering::Release);
        let enabled = u8::from(lockon) * USE_LOCKON | u8::from(pickup) * USE_PICKUP;
        ACTIONS.store(enabled, Ordering::Release);
        if lockon {
            println!("keyboard       {} supplies the native on-foot interaction", profile::current().binding(InputAction::Interact));
        }
        if pickup {
            println!("keyboard       {} picks up eligible on-foot objects", profile::current().binding(InputAction::Pickup));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickup_uses_only_the_distinct_active_on_foot_action() {
        let only_pickup = |id| id == PICKUP;
        assert_eq!(
            action_for_state(USE_PICKUP, only_pickup, || true),
            Some(Action::Pickup)
        );
        assert_eq!(action_for_state(USE_PICKUP, only_pickup, || false), None);
        assert_eq!(action_for_state(0, only_pickup, || true), None);

        for protected in PICKUP_EXCLUSIONS {
            assert_eq!(
                action_for_state(USE_PICKUP, |id| id == protected, || true),
                None
            );
            assert_eq!(
                action_for_state(USE_PICKUP, |id| id == PICKUP || id == protected, || true),
                None
            );
        }
    }

    #[test]
    fn pickup_option_preserves_existing_lockon_and_device_switching() {
        assert_eq!(
            action_for_state(
                USE_LOCKON | USE_PICKUP,
                |id| id == INTERACT_LOCKON,
                || false
            ),
            Some(Action::LockOn)
        );
        assert_eq!(
            action_for_state(USE_PICKUP, |id| id == INTERACT_LOCKON, || false),
            None
        );
        assert_eq!(
            action_for_state(
                USE_LOCKON | USE_PICKUP,
                |id| id == PICKUP || id == INTERACT_LOCKON,
                || true
            ),
            Some(Action::LockOn)
        );
        assert_eq!(
            active_keyboard_samples(ActiveInput::Keyboard, CURRENT | PREVIOUS),
            3
        );
        assert_eq!(
            active_keyboard_samples(ActiveInput::Controller, CURRENT | PREVIOUS),
            0
        );
        assert_eq!(
            active_keyboard_samples(ActiveInput::Unknown, CURRENT | PREVIOUS),
            0
        );
    }

    #[test]
    fn lockon_keeps_separate_press_hold_release_samples() {
        let mut sample = PadSample { device: 0, bits: 0 };
        assert_eq!(sample.advance(1, [false, false]), 0);
        assert_eq!(sample.advance(1, [true, false]), CURRENT);
        assert_eq!(sample.advance(2, [false, false]), CURRENT);
        assert_eq!(sample.advance(1, [true, false]), CURRENT | PREVIOUS);
        assert_eq!(sample.advance(1, [false, false]), PREVIOUS);
        assert_eq!(sample.advance(1, [false, false]), 0);
        assert_eq!(sample.advance(2, [true, false]), CURRENT);
        assert_eq!(sample.advance(2, [false, true]), PREVIOUS | (CURRENT << 2));
        assert_eq!(sample.advance(2, [false, false]), PREVIOUS << 2);
    }

    #[test]
    fn lockon_preserves_native_row_and_uses_the_requested_sample() {
        let mut raw = [0xfeed; 12];
        raw[1] = 0.25_f32.to_bits();
        raw[3] = 0.5_f32.to_bits();
        assert_eq!(with_keyboard(raw, 0), raw);

        let mut pressed = raw;
        pressed[1] = 1.0_f32.to_bits();
        assert_eq!(with_keyboard(raw, CURRENT), pressed);

        let mut released = raw;
        released[3] = 1.0_f32.to_bits();
        assert_eq!(with_keyboard(raw, PREVIOUS), released);
    }
}
