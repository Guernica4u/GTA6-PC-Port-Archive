//! Foreground mouse controls for the verified Scarlett 1.0.2.1 gameplay pad.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU16, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use victory_hle::modules::gameinput::{
    MouseGameplayInput, MouseGameplayMode, MouseWheelStep, mouse_gameplay_has_focus,
    mouse_sensitivity, sample_mouse_gameplay, set_title_pointer_menu,
};
use victory_core::input::profile::{self, Action as InputAction};
use victory_loader::LoadedImage;
use victory_loader::startup_calls::{StartupCall, redirect};

use super::lockon::{
    active_action_state, lockon_image_base, mouse_gameplay_context, on_foot_gameplay_player,
};
use super::{mouse_timing, scope_zoom, weapon_wheel};

const WRITE_PAD_CONNECTED: u32 = 0x01bc_f170;
const PED_WEAPONS: usize = 0x04f3_a1d0;
const READ_HAND_WEAPON: usize = 0x00c1_7400;
const UNARMED: u32 = 0xa271_9263;
const GROUP_UNARMED: u32 = 0xa00f_c1e4;
const GROUP_MELEE: u32 = 0xd493_21d4;

static DEVICE: AtomicUsize = AtomicUsize::new(0);
static BUTTONS: AtomicU16 = AtomicU16::new(0);
static STATES: [AtomicUsize; ACTIONS.len()] = [const { AtomicUsize::new(0) }; ACTIONS.len()];
static TRACE: OnceLock<bool> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Action {
    Aim,
    Attack,
    MeleeAttack,
    WheelNext,
    WheelPrevious,
    ZoomIn,
    ZoomOut,
}

const ACTIONS: [Action; 7] = [
    Action::Aim,
    Action::Attack,
    Action::MeleeAttack,
    Action::WheelNext,
    Action::WheelPrevious,
    Action::ZoomIn,
    Action::ZoomOut,
];

impl Action {
    fn id(self) -> usize {
        // The original action-hash table at RVA 0x0619_1240.
        match self {
            Self::Aim => 0x034,
            Self::Attack => 0x033,
            Self::MeleeAttack => 0x135,
            Self::WheelNext => weapon_wheel::NEXT_WEAPON,
            Self::WheelPrevious => weapon_wheel::PREVIOUS_WEAPON,
            Self::ZoomIn => scope_zoom::ZOOM_IN,
            Self::ZoomOut => scope_zoom::ZOOM_OUT,
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }

    unsafe fn state(self, base: usize) -> Option<*const u8> {
        match self {
            Self::WheelNext | Self::WheelPrevious => unsafe {
                weapon_wheel::routed_selection(base, self.id())
            },
            Self::ZoomIn | Self::ZoomOut => {
                let player = unsafe { on_foot_gameplay_player(base) }?;
                if !unsafe { scope_zoom::scope_active(base, player) } {
                    return None;
                }
                unsafe { scope_zoom::zoom_action_state(base, self.id()) }
            }
            _ => unsafe { active_action_state(base, self.id()) },
        }
    }
}

pub(super) fn enabled() -> bool {
    std::env::var_os("VICTORY_MOUSE_ACTIONS").is_some_and(|value| value == "1")
}

fn scope_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("VICTORY_MOUSE_SCOPE_ZOOM").is_some_and(|v| v == "1"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HandWeapon {
    Unarmed,
    Melee,
    Other,
    Unknown,
}

fn weapon_group(id: u32, group: u32) -> HandWeapon {
    if matches!(id, 0 | UNARMED) || group == GROUP_UNARMED {
        HandWeapon::Unarmed
    } else if group == GROUP_MELEE {
        HandWeapon::Melee
    } else if group != 0 {
        HandWeapon::Other
    } else {
        HandWeapon::Unknown
    }
}

unsafe fn hand_weapons(base: usize, player: *const u8) -> Option<[HandWeapon; 2]> {
    let get_weapons: unsafe extern "system" fn(u32) -> *const u8 =
        unsafe { std::mem::transmute(base + PED_WEAPONS) };
    let weapon_in_hand: unsafe extern "system" fn(*const u8, u32) -> *const u8 =
        unsafe { std::mem::transmute(base + READ_HAND_WEAPON) };
    // The native HUD at 0xc170e4 uses the same local player's entity ID.
    let player_id = unsafe { player.add(0x9c).cast::<u32>().read_unaligned() };
    let weapons = unsafe { get_weapons(player_id) };
    if weapons.is_null() {
        return None;
    }
    // 0xc17108..0xc17128 reads both equipped slots using the same getter.
    let hands = unsafe { weapons.add(0x1410) };
    Some([0, 1].map(|slot| {
        let item = unsafe { weapon_in_hand(hands, slot) };
        if item.is_null() {
            return HandWeapon::Unarmed;
        }
        // 0xc173e0 reads the equipped ID at +0x20. The native caller at
        // 0x256398a follows this same getter's item+0x10 and compares the
        // group at definition+0x2c. 0x2469a53 checks GROUP_MELEE there.
        let id = unsafe { item.add(0x20).cast::<u32>().read_unaligned() };
        let definition = unsafe { item.add(0x10).cast::<*const u8>().read_unaligned() };
        let group = if definition.is_null() {
            0
        } else {
            unsafe { definition.add(0x2c).cast::<u32>().read_unaligned() }
        };
        weapon_group(id, group)
    }))
}

fn attack_action(weapons: Option<[HandWeapon; 2]>) -> Option<Action> {
    let [primary, secondary] = weapons?;
    let attack = if primary == HandWeapon::Unarmed {
        secondary
    } else {
        primary
    };
    match attack {
        HandWeapon::Unarmed | HandWeapon::Melee => Some(Action::MeleeAttack),
        HandWeapon::Other => Some(Action::Attack),
        HandWeapon::Unknown => None,
    }
}

fn route_buttons(mouse: MouseGameplayInput, weapons: Option<[HandWeapon; 2]>) -> u8 {
    let aim = if mouse.aim { Action::Aim.bit() } else { 0 };
    let attack = match attack_action(weapons) {
        Some(Action::Attack) if mouse.fire => Action::Attack.bit(),
        Some(Action::MeleeAttack) if mouse.punch => Action::MeleeAttack.bit(),
        _ => 0,
    };
    aim | attack
}

fn route_scroll(mode: MouseGameplayMode, wheel: Option<MouseWheelStep>) -> u8 {
    match (mode, wheel) {
        (MouseGameplayMode::WeaponWheel, Some(MouseWheelStep::Up)) => Action::WheelNext.bit(),
        (MouseGameplayMode::WeaponWheel, Some(MouseWheelStep::Down)) => Action::WheelPrevious.bit(),
        (MouseGameplayMode::Scope, Some(MouseWheelStep::Up)) => Action::ZoomIn.bit(),
        (MouseGameplayMode::Scope, Some(MouseWheelStep::Down)) => Action::ZoomOut.bit(),
        _ => 0,
    }
}

fn next_sample(before: u16, active: u8) -> u16 {
    ((before & 0xff) << 8) | u16::from(active)
}

fn previous_actions_in(mode: MouseGameplayMode) -> u16 {
    const COMBAT: u8 = Action::Aim.bit() | Action::Attack.bit() | Action::MeleeAttack.bit();
    let keep = match mode {
        MouseGameplayMode::Inactive => 0,
        MouseGameplayMode::OnFoot => COMBAT,
        MouseGameplayMode::Scope => COMBAT | Action::ZoomIn.bit() | Action::ZoomOut.bit(),
        MouseGameplayMode::WeaponWheel => Action::WheelNext.bit() | Action::WheelPrevious.bit(),
    };
    u16::from(keep)
}

fn row_samples(sample: u16, action: Action) -> u8 {
    let mask = u16::from(action.bit());
    u8::from(sample & mask != 0) | (u8::from(sample & (mask << 8) != 0) << 1)
}

unsafe fn publish(base: usize, mode: MouseGameplayMode, active: u8, player: *const u8) {
    if mouse_timing::selected() {
        let eligible = matches!(mode, MouseGameplayMode::OnFoot | MouseGameplayMode::Scope)
            && mouse_gameplay_has_focus();
        let player = if eligible { player } else { std::ptr::null() };
        unsafe {
            mouse_timing::record(
                base,
                player,
                [
                    active & Action::Aim.bit() != 0,
                    active & Action::Attack.bit() != 0,
                    active & Action::MeleeAttack.bit() != 0,
                ],
            )
        };
    }
    let before = BUTTONS.load(Ordering::Relaxed);
    let samples = next_sample(before & previous_actions_in(mode), active);
    if before == 0 && samples == 0 {
        return;
    }
    for action in ACTIONS {
        let state = if row_samples(samples, action) == 0 {
            0
        } else {
            unsafe { action.state(base) }.map_or(0, |ptr| ptr as usize)
        };
        STATES[action as usize].store(state, Ordering::Relaxed);
    }
    BUTTONS.store(samples, Ordering::Release);
}

// The original writes the connected flag before all buttons/sticks on each
// native pad update. ONE_PAD supplies only the primary logical device.
unsafe extern "system" fn observe_pad_update(device: *mut u8, connected: u8) {
    let base = lockon_image_base();
    let native: unsafe extern "system" fn(*mut u8, u8) =
        unsafe { std::mem::transmute(base + WRITE_PAD_CONNECTED as usize) };
    unsafe { native(device, connected) };
    let id = device as usize;
    if connected == 0 {
        if DEVICE
            .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            sample_mouse_gameplay(MouseGameplayMode::Inactive);
            unsafe { weapon_wheel::sample(base, false) };
            unsafe { publish(base, MouseGameplayMode::Inactive, 0, std::ptr::null()) };
        }
        return;
    }
    if DEVICE
        .compare_exchange(0, id, Ordering::AcqRel, Ordering::Acquire)
        .is_err_and(|owner| owner != id)
    {
        return;
    }

    let (interface_owns_input, player) = unsafe { mouse_gameplay_context(base) };
    set_title_pointer_menu(interface_owns_input);
    let Some(player) = player else {
        sample_mouse_gameplay(MouseGameplayMode::Inactive);
        unsafe { weapon_wheel::sample(base, false) };
        unsafe { publish(base, MouseGameplayMode::Inactive, 0, std::ptr::null()) };
        return;
    };
    let mode = if unsafe { weapon_wheel::sample(base, true) } {
        MouseGameplayMode::WeaponWheel
    } else if scope_enabled() && unsafe { scope_zoom::scope_active(base, player) } {
        MouseGameplayMode::Scope
    } else {
        MouseGameplayMode::OnFoot
    };
    let mouse = sample_mouse_gameplay(mode);
    let weapons = if mouse.fire || mouse.punch {
        unsafe { hand_weapons(base, player) }
    } else {
        None
    };
    unsafe {
        publish(
            base,
            mode,
            route_scroll(mode, mouse.wheel) | route_buttons(mouse, weapons),
            player,
        )
    };
}

/// Returns current/previous synthetic values only for a live native action
/// row of player one. The original per-action gates still run after the read.
pub(super) unsafe fn for_action_row(base: usize, state: *const u8) -> u8 {
    let sample = BUTTONS.load(Ordering::Acquire);
    if sample == 0 || state.is_null() {
        return 0;
    }
    for action in ACTIONS {
        let bits = row_samples(sample, action);
        if bits != 0
            && STATES[action as usize].load(Ordering::Relaxed) == state as usize
            && mouse_gameplay_has_focus()
            && unsafe { action.state(base) } == Some(state)
        {
            trace(action, bits);
            return bits;
        }
    }
    0
}

/// Mouse inputs for native gameplay consumers that read the action's digital
/// state directly instead of its current/previous analog rows.
pub(super) unsafe fn mouse_digital_pressed(
    base: usize,
    state: *const u8,
    action_id: usize,
) -> bool {
    let action = match action_id {
        0x034 => Action::Aim,
        0x135 => Action::MeleeAttack,
        _ => return false,
    };
    let bits = row_samples(BUTTONS.load(Ordering::Acquire), action);
    if bits & 1 == 0
        || state.is_null()
        || STATES[action as usize].load(Ordering::Relaxed) != state as usize
        || !mouse_gameplay_has_focus()
        || (unsafe { active_action_state(base, action.id()) }) != Some(state)
    {
        return false;
    }
    let Some(player) = (unsafe { on_foot_gameplay_player(base) }) else {
        return false;
    };
    if action == Action::MeleeAttack
        && attack_action(unsafe { hand_weapons(base, player) }) != Some(Action::MeleeAttack)
    {
        return false;
    }
    trace(action, bits);
    true
}

/// Revalidate the live context when a native timed consumer reads the ring.
pub(super) unsafe fn timed_action_allowed(
    base: usize,
    state: *const u8,
    player: *const u8,
    action_id: usize,
) -> bool {
    if !mouse_gameplay_has_focus()
        || unsafe { on_foot_gameplay_player(base) } != Some(player)
        || unsafe { active_action_state(base, action_id) } != Some(state)
        || unsafe { weapon_wheel::routed_selection(base, weapon_wheel::NEXT_WEAPON) }.is_some()
    {
        return false;
    }
    if action_id == Action::Aim.id() {
        return true;
    }
    attack_action(unsafe { hand_weapons(base, player) })
        .is_some_and(|action| action.id() == action_id)
}

fn trace(action: Action, bits: u8) {
    if !*TRACE
        .get_or_init(|| std::env::var_os("VICTORY_MOUSE_ACTIONS_TRACE").is_some_and(|v| v == "1"))
    {
        return;
    }
    static LAST: [AtomicU8; ACTIONS.len()] = [const { AtomicU8::new(0) }; ACTIONS.len()];
    let current = bits & 1 != 0;
    if LAST[action as usize].swap(u8::from(current), Ordering::Relaxed) != u8::from(current) {
        let utc_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        eprintln!(
            "VIctory native mouse action: utc_us={utc_us} action={action:?} id={:#x} pressed={current} previous={}",
            action.id(),
            bits & 2 != 0
        );
    }
}

/// # Safety
/// The loader has verified the fixed title and installed the common action-row
/// hooks. The newly mapped title has not executed any instructions yet.
pub(super) unsafe fn install(image: &LoadedImage) -> Result<()> {
    if !enabled() {
        return Ok(());
    }
    let sites = [StartupCall {
        call_rva: 0x007a_42ec,
        original_rva: WRITE_PAD_CONNECTED,
        replacement: observe_pad_update as *const c_void,
    }];
    unsafe { redirect(image, &sites) }?;
    let keys = profile::current();
    println!("mouse actions  {} aims, {} fires, {} uses melee (mouse sensitivity {:.2}×)",
        keys.binding(InputAction::Aim), keys.binding(InputAction::Fire),
        keys.binding(InputAction::Punch), mouse_sensitivity());
    if scope_enabled() {
        println!("scope          {} zooms in, {} zooms out",
            keys.binding(InputAction::ZoomIn), keys.binding(InputAction::ZoomOut));
    }
    weapon_wheel::print_options();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_click_routes_to_only_one_eligible_action() {
        use HandWeapon::{Melee, Other, Unarmed, Unknown};
        let mouse = MouseGameplayInput {
            aim: true,
            fire: true,
            punch: true,
            wheel: None,
        };
        assert_eq!(route_buttons(mouse, None), Action::Aim.bit());
        assert_eq!(
            route_buttons(mouse, Some([Unarmed, Unarmed])),
            Action::Aim.bit() | Action::MeleeAttack.bit()
        );
        assert_eq!(
            route_buttons(mouse, Some([Unarmed, Other])),
            Action::Aim.bit() | Action::Attack.bit()
        );
        assert_eq!(
            route_buttons(mouse, Some([Melee, Other])),
            Action::Aim.bit() | Action::MeleeAttack.bit()
        );
        assert_eq!(
            route_buttons(mouse, Some([Unknown, Other])),
            Action::Aim.bit()
        );
        assert_eq!(
            route_buttons(MouseGameplayInput::default(), Some([Unarmed, Unarmed])),
            0
        );
        assert_eq!(route_buttons(MouseGameplayInput { fire: false, ..mouse }, Some([Other, Unarmed])), Action::Aim.bit());
        assert_eq!(route_buttons(MouseGameplayInput { punch: false, ..mouse }, Some([Unarmed, Unarmed])), Action::Aim.bit());
    }

    #[test]
    fn the_title_melee_group_differs_from_a_gun_and_unavailable_definition() {
        assert_eq!(weapon_group(0, 0), HandWeapon::Unarmed);
        assert_eq!(weapon_group(UNARMED, 0), HandWeapon::Unarmed);
        assert_eq!(weapon_group(1, GROUP_UNARMED), HandWeapon::Unarmed);
        assert_eq!(weapon_group(1, GROUP_MELEE), HandWeapon::Melee);
        assert_eq!(weapon_group(1, 0x18d5_fa97), HandWeapon::Other);
        assert_eq!(weapon_group(1, 0), HandWeapon::Unknown);
    }

    #[test]
    fn native_actions_keep_distinct_press_hold_and_release_samples() {
        let action = Action::MeleeAttack;
        let press = next_sample(0, action.bit());
        let hold = next_sample(press, action.bit());
        let release = next_sample(hold, 0);
        assert_eq!(
            [press, hold, release, next_sample(release, 0)].map(|s| row_samples(s, action)),
            [1, 3, 2, 0]
        );
        assert_eq!(row_samples(hold, Action::Attack), 0);
    }

    #[test]
    fn wheel_detents_route_only_to_the_active_selection_or_scope() {
        use MouseGameplayMode::{Inactive, OnFoot, Scope, WeaponWheel};
        use MouseWheelStep::{Down, Up};
        assert_eq!(route_scroll(Inactive, Some(Up)), 0);
        assert_eq!(route_scroll(OnFoot, Some(Down)), 0);
        assert_eq!(route_scroll(Scope, Some(Up)), Action::ZoomIn.bit());
        assert_eq!(route_scroll(Scope, Some(Down)), Action::ZoomOut.bit());
        assert_eq!(route_scroll(WeaponWheel, None), 0);
        assert_eq!(route_scroll(WeaponWheel, Some(Up)), Action::WheelNext.bit());
        assert_eq!(
            route_scroll(WeaponWheel, Some(Down)),
            Action::WheelPrevious.bit()
        );
    }

    #[test]
    fn mode_changes_cancel_old_wheel_and_scope_pulses() {
        use MouseGameplayMode::{Inactive, OnFoot, Scope, WeaponWheel};
        let wheel = next_sample(0, Action::WheelNext.bit());
        let zoom = next_sample(0, Action::ZoomOut.bit());
        let keyboard = next_sample(0, Action::Attack.bit());
        assert_eq!(next_sample(wheel & previous_actions_in(Scope), 0), 0);
        assert_eq!(next_sample(zoom & previous_actions_in(WeaponWheel), 0), 0);
        assert_eq!(next_sample(zoom & previous_actions_in(OnFoot), 0), 0);
        assert_eq!(next_sample(keyboard & previous_actions_in(Inactive), 0), 0);
    }
}
