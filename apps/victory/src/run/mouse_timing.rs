//! Mouse edges for the native timed combat queries.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use victory_loader::LoadedImage;
use victory_loader::startup_calls::{StartupCall, redirect};

use super::lockon::{active_action_state, lockon_image_base};
use super::mouse_controls::{enabled, timed_action_allowed};

const ACTIONS: [usize; 3] = [0x034, 0x033, 0x135];
const EVENT_TIME: usize = 0x09ab_1c74;
const NATIVE_ROW: u32 = 0x04db_8680;
const MAX_MOUSE_SAMPLES: usize = 32;
const MAX_NATIVE_SAMPLES: usize = u8::MAX as usize;

type Row = [u32; 12];

#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    ms: u32,
    pressed: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Event {
    source: u32,
    value: f32,
    ms: u32,
}

#[derive(Default)]
struct History {
    player: usize,
    state: usize,
    samples: VecDeque<Sample>,
}

impl History {
    fn record(&mut self, player: usize, state: usize, ms: u32, pressed: bool) {
        if player != self.player
            || state != self.state
            || self.samples.back().is_some_and(|last| ms < last.ms)
        {
            self.samples.clear();
            self.player = player;
            self.state = state;
        }
        if player == 0 || state == 0 || ms == 0 {
            self.samples.clear();
            return;
        }
        // Require a real released sample in this action context before
        // adding an edge. A button already held on entry supplies no edge.
        if self.samples.is_empty() && pressed {
            return;
        }
        if let Some(last) = self.samples.back_mut() {
            if last.ms == ms {
                last.pressed = pressed;
                return;
            }
            if last.pressed == pressed {
                return;
            }
        }
        if self.samples.len() == MAX_MOUSE_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { ms, pressed });
    }
}

static HISTORY: Mutex<[History; ACTIONS.len()]> = Mutex::new(
    [const {
        History {
            player: 0,
            state: 0,
            samples: VecDeque::new(),
        }
    }; ACTIONS.len()],
);

struct Shadow {
    row: Row,
    events: Vec<Event>,
}

thread_local! {
    // The three native callers use the returned row immediately. They keep
    // the state object and its eligibility flags in RSI throughout the read.
    static SHADOW: RefCell<Shadow> = const { RefCell::new(Shadow { row: [0; 12], events: Vec::new() }) };
}

pub(super) fn selected() -> bool {
    static TIMED: OnceLock<bool> = OnceLock::new();
    *TIMED.get_or_init(|| {
        std::env::var_os("VICTORY_MOUSE_TIMED_ACTIONS").is_some_and(|value| value == "1")
    })
}

unsafe fn title_clock(base: usize) -> u32 {
    // 0xa04cb0 updates this millisecond clock from QPC. The original
    // 0x4b7f80 uses the same clock for its timed input ranges.
    unsafe { ((base + EVENT_TIME) as *const u32).read_volatile() }
}

/// # Safety
/// Called from the primary pad update of the verified title image.
pub(super) unsafe fn record(base: usize, player: *const u8, held: [bool; ACTIONS.len()]) {
    let (states, ms) = if !player.is_null() {
        (
            ACTIONS.map(|id| unsafe { active_action_state(base, id) }.map_or(0, |p| p as usize)),
            unsafe { title_clock(base) },
        )
    } else {
        ([0; ACTIONS.len()], 0)
    };
    let mut histories = HISTORY.lock().unwrap_or_else(|err| err.into_inner());
    for ((history, state), pressed) in histories.iter_mut().zip(states).zip(held) {
        history.record(player as usize, state, ms, pressed);
    }
}

fn value_with_mouse(native: f32, pressed: bool) -> f32 {
    if pressed { native.max(1.0) } else { native }
}

fn add_mouse_edge(output: &mut Vec<Event>, actual: Event, sample: Sample, held: &mut bool) {
    let before = value_with_mouse(actual.value, *held);
    *held = sample.pressed;
    let value = value_with_mouse(actual.value, *held);
    if value != before {
        output.push(Event {
            source: actual.source,
            value,
            ms: sample.ms,
        });
    }
}

fn combine(native: &[Event], mouse: &VecDeque<Sample>, output: &mut Vec<Event>) {
    let Some(&first) = native.first() else { return };
    let mut actual = first;
    let mut held = false;
    let mut samples = mouse.iter().peekable();
    for &event in native {
        while let Some(sample) = samples.next_if(|sample| sample.ms < event.ms) {
            add_mouse_edge(output, actual, *sample, &mut held);
        }
        if let Some(sample) = samples.next_if(|sample| sample.ms == event.ms) {
            held = sample.pressed;
        }
        actual = event;
        output.push(Event {
            value: value_with_mouse(actual.value, held),
            ..actual
        });
    }
    for sample in samples {
        add_mouse_edge(output, actual, *sample, &mut held);
    }
}

unsafe fn native_ring(row: &Row, now: u32) -> Option<Vec<Event>> {
    let cap = row[7] as usize;
    let head = row[10] as usize;
    if !(2..=MAX_NATIVE_SAMPLES).contains(&cap) || row[6] as usize != cap || head >= cap {
        return None;
    }
    let pointer = (u64::from(row[8]) | u64::from(row[9]) << 32) as *const Event;
    if pointer.is_null() {
        return None;
    }
    let events: Vec<_> = (1..=cap)
        .map(|offset| unsafe { pointer.add((head + offset) % cap).read_unaligned() })
        .collect();
    // The original compares its millisecond stamps in unsigned order.
    if events
        .iter()
        .any(|event| event.ms > now || !event.value.is_finite())
        || events.windows(2).any(|pair| pair[0].ms > pair[1].ms)
    {
        return None;
    }
    Some(events)
}

unsafe fn timed_row(original: *const Row, state: *const u8, base: usize) -> Option<*const Row> {
    if state.is_null()
        || unsafe { state.add(0x45).read() } != 1
        || unsafe { state.add(0x46).read() } & 1 != 0
    {
        return None;
    }

    let histories = HISTORY.lock().unwrap_or_else(|err| err.into_inner());
    let mut matching = histories
        .iter()
        .zip(ACTIONS)
        .filter(|(h, _)| h.state == state as usize);
    let (history, id) = matching.next()?;
    if matching.next().is_some() || !history.samples.iter().any(|sample| sample.pressed) {
        return None;
    }
    let samples = history.samples.clone();
    let player = history.player as *const u8;
    drop(histories);
    if !unsafe { timed_action_allowed(base, state, player, id) } {
        return None;
    }

    let now = unsafe { title_clock(base) };
    if samples.back().is_none_or(|sample| sample.ms > now) {
        return None;
    }
    let row = unsafe { original.read_unaligned() };
    let current = f32::from_bits(row[1]);
    if !current.is_finite() {
        return None;
    }
    let native = unsafe { native_ring(&row, now) }?;
    if row != unsafe { original.read_unaligned() } {
        return None;
    }
    let mut row = row;
    row[1] =
        value_with_mouse(current, samples.back().is_some_and(|sample| sample.pressed)).to_bits();
    SHADOW.with(|shadow| {
        let mut shadow = shadow.borrow_mut();
        shadow.events.clear();
        combine(&native, &samples, &mut shadow.events);
        // Keep the original ring capacity and the same newest-entry order.
        // Include the newest events if the real and mouse edges fill the ring.
        let cap = native.len();
        let start = shadow.events.len() - cap;
        let events = shadow.events.as_ptr().wrapping_add(start) as usize as u64;
        row[8] = events as u32;
        row[9] = (events >> 32) as u32;
        row[10] = (cap - 1) as u32;
        shadow.row = row;
        Some(&raw const shadow.row)
    })
}

#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
unsafe extern "system" fn select_timed_row(_: u32, _: *const Row, _: u32) -> *const Row {
    std::arch::naked_asm!("mov r9, rsi", "jmp {}", sym select_timed_row_with_state);
}

unsafe extern "system" fn select_timed_row_with_state(
    count: u32,
    rows: *const Row,
    index: u32,
    state: *const u8,
) -> *const Row {
    let base = lockon_image_base();
    let native: unsafe extern "system" fn(u32, *const Row, u32) -> *const Row =
        unsafe { std::mem::transmute(base + NATIVE_ROW as usize) };
    let original = unsafe { native(count, rows, index) };
    if index != 0 || count == 0 || original.is_null() {
        return original;
    }
    unsafe { timed_row(original, state, base) }.unwrap_or(original)
}

/// # Safety
/// The caller has verified the title image and has not started the game.
pub(super) unsafe fn install(image: &LoadedImage) -> Result<()> {
    if !enabled() || !selected() {
        return Ok(());
    }
    let sites = [
        0x00be_9c60, // A transition within the requested window
        0x004b_7e1e, // A held value throughout the requested window
        0x00b6_0298, // The count of transitions in the requested window
    ]
    .map(|call_rva| StartupCall {
        call_rva,
        original_rva: NATIVE_ROW,
        replacement: select_timed_row as *const c_void,
    });
    unsafe { redirect(image, &sites) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(entries: &[(u32, f32)]) -> Vec<Event> {
        entries
            .iter()
            .map(|&(ms, value)| Event {
                source: 0x23,
                value,
                ms,
            })
            .collect()
    }

    fn edges(entries: &[(u32, bool)]) -> VecDeque<Sample> {
        entries
            .iter()
            .map(|&(ms, pressed)| Sample { ms, pressed })
            .collect()
    }

    #[test]
    fn history_needs_a_released_sample_in_each_binding_context() {
        let mut history = History::default();
        history.record(1, 10, 690, true);
        assert!(history.samples.is_empty());
        history.record(1, 10, 700, false);
        history.record(1, 10, 950, true);
        history.record(1, 10, 960, true);
        history.record(1, 10, 980, false);
        assert_eq!(
            history.samples,
            edges(&[(700, false), (950, true), (980, false)])
        );
        history.record(1, 11, 990, true);
        assert!(history.samples.is_empty());
        history.record(1, 11, 1000, false);
        history.record(1, 11, 10, true);
        assert!(history.samples.is_empty());
        history.record(1, 11, 20, false);
        history.record(1, 11, 40, true);
        history.record(2, 11, 50, true);
        assert!(history.samples.is_empty());
    }

    #[test]
    fn combines_mouse_and_original_device_without_replacing_its_hold() {
        let native = inputs(&[(700, 0.0), (900, 1.0), (960, 1.0), (990, 0.0)]);
        let mouse = edges(&[(705, false), (950, true), (980, false)]);
        let mut output = Vec::new();
        combine(&native, &mouse, &mut output);
        assert_eq!(native, output);

        let mouse = edges(&[(705, false), (950, true), (1005, false)]);
        output.clear();
        combine(&native, &mouse, &mut output);
        assert_eq!(
            output,
            inputs(&[(700, 0.0), (900, 1.0), (960, 1.0), (990, 1.0), (1005, 0.0)])
        );
    }

    #[test]
    fn mouse_edge_retains_the_native_clock_and_respects_coincident_samples() {
        let native = inputs(&[(700, 0.0), (950, 0.0)]);
        let mouse = edges(&[(705, false), (950, true), (980, false)]);
        let mut output = Vec::new();
        combine(&native, &mouse, &mut output);
        assert_eq!(output, inputs(&[(700, 0.0), (950, 1.0), (980, 0.0)]));
    }
}
