//! Observation of the title's on-foot equipment-wheel actions.
//!
//! Open-wheel input alone does not authorize scrolling. Require a genuine
//! native read of the wheel's selection controls during the same hold.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use victory_core::input::profile::{self, Action as InputAction};

use super::lockon::active_action_state;

pub(super) const NEXT_WEAPON: usize = 0x024; // INPUT_SELECT_NEXT_WEAPON
pub(super) const PREVIOUS_WEAPON: usize = 0x025; // INPUT_SELECT_PREV_WEAPON
const OPEN_WHEEL: usize = 0x04a; // INPUT_OPEN_WHEEL_MENU
const OPEN: u8 = 1;
const NEXT_READ: u8 = 2;
const PREVIOUS_READ: u8 = 4;

// A layout that resolves a selection to one of these objects would not keep
// scrolling scoped to the equipment wheel.
const EXCLUDED: [usize; 8] = [
    0x018, // INPUT_SELECT_NEXT_WHEEL (equipment pages)
    0x04c, // INPUT_QUICK_SWAP_WEAPON (aiming without the wheel)
    0x1d2, // INPUT_PREV_WEAPON (world)
    0x1d3, // INPUT_NEXT_WEAPON (world)
    0x18c, // INPUT_FRONTEND_ACCEPT
    0x18d, // INPUT_FRONTEND_CANCEL
    0x052, // INPUT_SNIPER_ZOOM_IN_SECONDARY
    0x053, // INPUT_SNIPER_ZOOM_OUT_SECONDARY
];

struct Config {
    trace: bool,
    route: bool,
}

fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let enabled = |key| std::env::var_os(key).is_some_and(|value| value == "1");
        Config {
            trace: enabled("VICTORY_MOUSE_WHEEL_TRACE"),
            route: enabled("VICTORY_MOUSE_WEAPON_WHEEL"),
        }
    })
}

static OBSERVING: AtomicBool = AtomicBool::new(false);
static OPEN_ROW: AtomicUsize = AtomicUsize::new(0);
static NEXT_ROW: AtomicUsize = AtomicUsize::new(0);
static PREVIOUS_ROW: AtomicUsize = AtomicUsize::new(0);
static OBSERVED: AtomicU8 = AtomicU8::new(0);

pub(super) fn print_options() {
    let Config { trace, route } = *config();
    OBSERVING.store(trace || route, Ordering::Release);
    if route {
        let keys = profile::current();
        println!("weapon wheel   {} opens, {} selects next, {} selects previous",
            keys.binding(InputAction::WeaponWheel), keys.binding(InputAction::WheelNext),
            keys.binding(InputAction::WheelPrevious));
    } else if trace {
        println!("mouse wheel    observing native equipment selection");
    }
}

#[derive(Clone, Copy)]
enum Read {
    Closed,
    Open,
    Next,
    Previous,
}

fn advance_observation(before: u8, read: Read) -> u8 {
    match read {
        Read::Closed => 0,
        Read::Open => before | OPEN,
        Read::Next if before & OPEN != 0 => before | NEXT_READ,
        Read::Previous if before & OPEN != 0 => before | PREVIOUS_READ,
        _ => before,
    }
}

fn wheel_armed(bits: u8) -> bool {
    bits & OPEN != 0 && bits & (NEXT_READ | PREVIOUS_READ) != 0
}

fn observe(read: Read) {
    let Ok(before) = OBSERVED.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
        let new = advance_observation(old, read);
        (new != old).then_some(new)
    }) else {
        return;
    };
    if config().trace {
        let new = advance_observation(before, read);
        eprintln!(
            "VIctory native weapon wheel: open={} next_read={} prev_read={} mouse_eligible={}",
            new & OPEN != 0,
            new & NEXT_READ != 0,
            new & PREVIOUS_READ != 0,
            wheel_armed(new)
        );
    }
}

fn distinct_selection_rows(open: usize, mut state: impl FnMut(usize) -> usize) -> (usize, usize) {
    let next = state(NEXT_WEAPON);
    let previous = state(PREVIOUS_WEAPON);
    if next == 0 || previous == 0 || next == previous || next == open || previous == open {
        return (0, 0);
    }
    for id in EXCLUDED {
        let alias = state(id);
        if alias == next || alias == previous {
            return (0, 0);
        }
    }
    (next, previous)
}

unsafe fn selection_rows(base: usize, open: usize) -> (usize, usize) {
    distinct_selection_rows(open, |id| {
        unsafe { active_action_state(base, id) }.map_or(0, |ptr| ptr as usize)
    })
}

unsafe fn still_held(state: usize) -> bool {
    // The native current-value accessor uses this count, pointer and first
    // row value at 0x4db864f..0x4db865b. Our read only clears stale holds.
    let action = state as *const u8;
    let count = unsafe { action.add(4).cast::<u32>().read_unaligned() };
    let rows = unsafe { action.add(8).cast::<*const u32>().read_unaligned() };
    count != 0 && !rows.is_null() && f32::from_bits(unsafe { rows.add(1).read_unaligned() }) >= 0.5
}

/// Called from the primary native gameplay pad update.
pub(super) unsafe fn sample(base: usize, on_foot: bool) -> bool {
    let Config { trace, route } = *config();
    if !route && !trace {
        return false;
    }
    let open = if on_foot {
        unsafe { active_action_state(base, OPEN_WHEEL) }.map_or(0, |ptr| ptr as usize)
    } else {
        0
    };
    let changed = OPEN_ROW.swap(open, Ordering::AcqRel) != open;
    let held = open != 0 && unsafe { still_held(open) };
    if changed || !held {
        observe(Read::Closed);
    }
    let (next, previous) = if held {
        unsafe { selection_rows(base, open) }
    } else {
        (0, 0)
    };
    let next_changed = NEXT_ROW.swap(next, Ordering::AcqRel) != next;
    let previous_changed = PREVIOUS_ROW.swap(previous, Ordering::AcqRel) != previous;
    if next_changed || previous_changed {
        OBSERVED.fetch_and(OPEN, Ordering::AcqRel);
    }
    route && wheel_armed(OBSERVED.load(Ordering::Acquire))
}

/// Revalidate the action identity and live open value when processing a pulse.
pub(super) unsafe fn routed_selection(base: usize, id: usize) -> Option<*const u8> {
    if !wheel_armed(OBSERVED.load(Ordering::Acquire)) {
        return None;
    }
    let open = (unsafe { active_action_state(base, OPEN_WHEEL) })? as usize;
    if open != OPEN_ROW.load(Ordering::Acquire) || !unsafe { still_held(open) } {
        return None;
    }
    let (next, previous) = unsafe { selection_rows(base, open) };
    let candidate = match id {
        NEXT_WEAPON => next,
        PREVIOUS_WEAPON => previous,
        _ => return None,
    } as *const u8;
    (!candidate.is_null()).then_some(candidate)
}

/// Watches the row the game itself requested, before any mouse values merge.
pub(super) unsafe fn observe_native_row(state: *const u8, row: *const [u32; 12]) {
    let address = state as usize;
    if address == 0 || !OBSERVING.load(Ordering::Relaxed) {
        return;
    }
    if OPEN_ROW.load(Ordering::Acquire) == address {
        let current = f32::from_bits(unsafe { row.cast::<u32>().add(1).read_unaligned() });
        observe(if current >= 0.5 {
            Read::Open
        } else {
            Read::Closed
        });
    } else if NEXT_ROW.load(Ordering::Acquire) == address {
        observe(Read::Next);
    } else if PREVIOUS_ROW.load(Ordering::Acquire) == address {
        observe(Read::Previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_selection_reads_only_arm_while_the_game_reads_an_open_hold() {
        let before = advance_observation(0, Read::Next);
        assert!(!wheel_armed(before));
        let open = advance_observation(before, Read::Open);
        assert!(!wheel_armed(open));
        let select = advance_observation(open, Read::Previous);
        assert!(wheel_armed(select));
        let new_hold = advance_observation(advance_observation(select, Read::Closed), Read::Open);
        assert!(!wheel_armed(new_hold));
    }

    #[test]
    fn rejects_a_selection_shared_with_a_world_action_page_or_zoom() {
        assert_eq!(
            distinct_selection_rows(10, |id| match id {
                NEXT_WEAPON => 11,
                PREVIOUS_WEAPON => 12,
                _ => 0,
            }),
            (11, 12)
        );
        for alias in EXCLUDED {
            assert_eq!(
                distinct_selection_rows(10, |id| match id {
                    NEXT_WEAPON => 11,
                    PREVIOUS_WEAPON => 12,
                    _ if id == alias => 11,
                    _ => 0,
                }),
                (0, 0)
            );
        }
    }
}
