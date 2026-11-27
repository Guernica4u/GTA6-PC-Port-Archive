//! Physical input device selection and keyboard labels shared with the UI.

use std::sync::atomic::{AtomicU8, Ordering};

pub mod profile;

/// The device family that last supplied intentional physical input.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ActiveInput {
    #[default]
    Unknown = 0,
    Keyboard = 1,
    Controller = 2,
}

static ACTIVE_INPUT: AtomicU8 = AtomicU8::new(ActiveInput::Unknown as u8);

/// Returns the latest device family for selecting UI glyphs.
pub fn active_input() -> ActiveInput {
    match ACTIVE_INPUT.load(Ordering::Acquire) {
        1 => ActiveInput::Keyboard,
        2 => ActiveInput::Controller,
        _ => ActiveInput::Unknown,
    }
}

/// Publishes a real input change. The result says if the family changed.
pub fn note_input(device: ActiveInput) -> bool {
    let value = device as u8;
    ACTIVE_INPUT
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |previous| {
            (device != ActiveInput::Unknown && value != previous).then_some(value)
        })
        .is_ok()
}

/// GameInput gamepad button bit values.
pub mod pad_buttons {
    pub const MENU: u32 = 0x1;
    pub const VIEW: u32 = 0x2;
    pub const A: u32 = 0x4;
    pub const B: u32 = 0x8;
    pub const X: u32 = 0x10;
    pub const Y: u32 = 0x20;
    pub const UP: u32 = 0x40;
    pub const DOWN: u32 = 0x80;
    pub const LEFT: u32 = 0x100;
    pub const RIGHT: u32 = 0x200;
    pub const LEFT_SHOULDER: u32 = 0x400;
    pub const RIGHT_SHOULDER: u32 = 0x800;
    pub const LEFT_STICK: u32 = 0x1000;
    pub const RIGHT_STICK: u32 = 0x2000;
}

/// Gamepad controls and their keyboard labels, shared by input and prompts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PadControl {
    Button(u32),
    LeftX(i8),
    LeftY(i8),
    RightX(i8),
    RightY(i8),
    LeftTrigger,
    RightTrigger,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyboardBinding {
    pub vk: u8,
    pub label: &'static str,
    pub control: PadControl,
}

pub const KEYBOARD_PAD_BINDINGS: &[KeyboardBinding] = {
    use PadControl::*;
    use pad_buttons::*;
    &[
        KeyboardBinding {
            vk: 0x1b,
            label: "Esc",
            control: Button(MENU),
        },
        KeyboardBinding {
            vk: 0x09,
            label: "Tab",
            control: Button(VIEW),
        },
        KeyboardBinding {
            vk: 0x0d,
            label: "Enter",
            control: Button(A),
        },
        KeyboardBinding {
            vk: 0x08,
            label: "Backspace",
            control: Button(B),
        },
        KeyboardBinding {
            vk: b'X',
            label: "X",
            control: Button(X),
        },
        KeyboardBinding {
            vk: b'F',
            label: "F",
            control: Button(Y),
        },
        KeyboardBinding {
            vk: 0x26,
            label: "Up",
            control: Button(UP),
        },
        KeyboardBinding {
            vk: 0x28,
            label: "Down",
            control: Button(DOWN),
        },
        KeyboardBinding {
            vk: 0x25,
            label: "Left",
            control: Button(LEFT),
        },
        KeyboardBinding {
            vk: 0x27,
            label: "Right",
            control: Button(RIGHT),
        },
        KeyboardBinding {
            vk: b'Q',
            label: "Q",
            control: Button(LEFT_SHOULDER),
        },
        KeyboardBinding {
            vk: b'E',
            label: "E",
            control: Button(RIGHT_SHOULDER),
        },
        KeyboardBinding {
            vk: 0xa2,
            label: "Ctrl",
            control: Button(LEFT_STICK),
        },
        KeyboardBinding {
            vk: b'H',
            label: "H",
            control: Button(RIGHT_STICK),
        },
        KeyboardBinding {
            vk: b'A',
            label: "A",
            control: LeftX(-1),
        },
        KeyboardBinding {
            vk: b'D',
            label: "D",
            control: LeftX(1),
        },
        KeyboardBinding {
            vk: b'S',
            label: "S",
            control: LeftY(-1),
        },
        KeyboardBinding {
            vk: b'W',
            label: "W",
            control: LeftY(1),
        },
        KeyboardBinding {
            vk: b'J',
            label: "J",
            control: RightX(-1),
        },
        KeyboardBinding {
            vk: b'L',
            label: "L",
            control: RightX(1),
        },
        KeyboardBinding {
            vk: b'K',
            label: "K",
            control: RightY(-1),
        },
        KeyboardBinding {
            vk: b'I',
            label: "I",
            control: RightY(1),
        },
        KeyboardBinding {
            vk: b'Z',
            label: "Z",
            control: LeftTrigger,
        },
        KeyboardBinding {
            vk: b'C',
            label: "C",
            control: RightTrigger,
        },
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_input_only_publishes_family_changes() {
        assert!(!note_input(ActiveInput::Unknown));
        assert!(note_input(ActiveInput::Keyboard));
        assert_eq!(active_input(), ActiveInput::Keyboard);
        assert!(!note_input(ActiveInput::Keyboard));
        assert!(note_input(ActiveInput::Controller));
        assert_eq!(active_input(), ActiveInput::Controller);
        assert!(!note_input(ActiveInput::Unknown));
        assert_eq!(active_input(), ActiveInput::Controller);
    }
}
