//! Shared keyboard/mouse controls, read once when the title starts.

use std::fmt;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binding {
    Key(u8),
    Mouse(u8),
    WheelUp,
    WheelDown,
}

const KEY_NAMES: &[(&str, u8)] = &[
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0d),
    ("Esc", 0x1b),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2d),
    ("Delete", 0x2e),
    ("Shift", 0x10),
    ("Ctrl", 0x11),
    ("Alt", 0x12),
];

const MOUSE_NAMES: &[&str] = &[
    "MouseLeft",
    "MouseRight",
    "MouseMiddle",
    "MouseX1",
    "MouseX2",
];

impl Binding {
    fn parse(value: &str) -> Option<Self> {
        if let Some(&(_, vk)) = KEY_NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(value))
        {
            return Some(Self::Key(vk));
        }
        if let Some(button) = MOUSE_NAMES
            .iter()
            .position(|name| name.eq_ignore_ascii_case(value))
        {
            return Some(Self::Mouse(button as u8));
        }
        let key = value.to_ascii_uppercase();
        let virtual_key = match key.as_str() {
            "WHEELUP" => return Some(Self::WheelUp),
            "WHEELDOWN" => return Some(Self::WheelDown),
            name if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() => {
                name.as_bytes()[0]
            }
            name => {
                let number = name.strip_prefix('F')?;
                if number.starts_with('0') || !number.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let number: u8 = number.parse().ok()?;
                if !(1..=12).contains(&number) {
                    return None;
                }
                0x6f + number
            }
        };
        Some(Self::Key(virtual_key))
    }

    pub const fn is_key(self, vk: u8) -> bool {
        match self {
            Self::Key(0x10) => matches!(vk, 0x10 | 0xa0 | 0xa1),
            Self::Key(0x11) => matches!(vk, 0x11 | 0xa2 | 0xa3),
            Self::Key(0x12) => matches!(vk, 0x12 | 0xa4 | 0xa5),
            Self::Key(key) => key == vk,
            _ => false,
        }
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Key(key @ 0x70..=0x7b) => write!(output, "F{}", key - 0x6f),
            Self::Key(key) if key.is_ascii_alphanumeric() => write!(output, "{}", char::from(key)),
            Self::Key(key) => {
                if let Some((name, _)) = KEY_NAMES.iter().find(|(_, vk)| *vk == key) {
                    output.write_str(name)
                } else {
                    write!(output, "VK {key:#04x}")
                }
            }
            Self::Mouse(button) => {
                output.write_str(MOUSE_NAMES.get(button as usize).unwrap_or(&"Mouse"))
            }
            Self::WheelUp => output.write_str("WheelUp"),
            Self::WheelDown => output.write_str("WheelDown"),
        }
    }
}

use Binding::{Key, Mouse, WheelDown, WheelUp};

macro_rules! actions {
    ($( $action:ident = $name:literal, $default:expr; )+) => {
        #[repr(usize)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum Action { $( $action, )+ }

        impl Action {
            const ALL: &[Self] = &[ $( Self::$action, )+ ];

            fn parse(name: &str) -> Option<Self> {
                match name {
                    $( $name => Some(Self::$action), )+
                    _ => None,
                }
            }
        }

        const DEFAULT_BINDINGS: [Binding; Action::ALL.len()] = [$( $default, )+];
    };
}

actions! {
    Interact = "interact", Key(b'F');
    Pickup = "pickup", Key(b'F');
    Aim = "aim", Mouse(1);
    Fire = "fire", Mouse(0);
    Punch = "punch", Mouse(0);
    ZoomIn = "zoom_in", WheelUp;
    ZoomOut = "zoom_out", WheelDown;
    WheelNext = "wheel_next", WheelUp;
    WheelPrevious = "wheel_previous", WheelDown;
    MoveForward = "move_forward", Key(b'W');
    MoveBack = "move_back", Key(b'S');
    MoveLeft = "move_left", Key(b'A');
    MoveRight = "move_right", Key(b'D');
    WeaponWheel = "weapon_wheel", Key(b'Q');
    BoatAccelerate = "boat_accelerate", Key(b'W');
    BoatBrake = "boat_brake", Key(b'S');
    BoatHandbrake = "boat_handbrake", Key(b' ');
}

impl Action {
    fn accepts(self, binding: Binding) -> bool {
        match self {
            Self::ZoomIn | Self::ZoomOut | Self::WheelNext | Self::WheelPrevious => true,
            Self::Aim | Self::Fire | Self::Punch => matches!(binding, Key(_) | Mouse(_)),
            _ => matches!(binding, Key(_)),
        }
    }
}

#[derive(Debug)]
pub struct Profile {
    bindings: [Binding; Action::ALL.len()],
    sensitivity: Option<f32>,
}

impl Profile {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut profile = DEFAULT;
        let mut seen = [false; Action::ALL.len()];
        for (index, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let bad = |reason: &str| format!("line {}: {reason}", index + 1);
            let (name, value) = line
                .split_once('=')
                .ok_or_else(|| bad("expected action=value"))?;
            let (name, value) = (name.trim(), value.trim());
            if name == "mouse_sensitivity" {
                if profile.sensitivity.is_some() {
                    return Err(bad("duplicate mouse_sensitivity"));
                }
                profile.sensitivity = Some(
                    value
                        .parse::<f32>()
                        .ok()
                        .filter(|value| (0.05..=10.0).contains(value))
                        .ok_or_else(|| bad("mouse_sensitivity must be between 0.05 and 10"))?,
                );
                continue;
            }
            let action = Action::parse(name).ok_or_else(|| bad("unknown action"))?;
            let binding = Binding::parse(value)
                .filter(|&binding| action.accepts(binding))
                .ok_or_else(|| bad("unsupported control for this action"))?;
            let index = action as usize;
            if std::mem::replace(&mut seen[index], true) {
                return Err(bad("duplicate action"));
            }
            profile.bindings[index] = binding;
        }
        if profile.binding(Action::ZoomIn) == profile.binding(Action::ZoomOut) {
            return Err("zoom_in and zoom_out need distinct controls".into());
        }
        if profile.binding(Action::WheelNext) == profile.binding(Action::WheelPrevious) {
            return Err("wheel_next and wheel_previous need distinct controls".into());
        }
        Ok(profile)
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Self::parse(&text)
    }

    pub const fn binding(&self, action: Action) -> Binding {
        self.bindings[action as usize]
    }

    pub const fn sensitivity(&self) -> Option<f32> {
        self.sensitivity
    }

    pub fn claims_key(&self, vk: u8) -> bool {
        self.bindings.iter().any(|&binding| binding.is_key(vk))
    }
}

const DEFAULT: Profile = Profile {
    bindings: DEFAULT_BINDINGS,
    sensitivity: None,
};

pub fn configured() -> Option<&'static Profile> {
    static PROFILE: LazyLock<Option<Profile>> = LazyLock::new(|| {
        let path = std::env::var_os("VICTORY_INPUT_BINDINGS")?;
        let path = Path::new(&path);
        match Profile::read(path) {
            Ok(profile) => Some(profile),
            Err(error) => {
                eprintln!(
                    "Input profile {}: {error}. Using default controls.",
                    path.display()
                );
                None
            }
        }
    });
    PROFILE.as_ref()
}

pub fn current() -> &'static Profile {
    static DEFAULT_PROFILE: Profile = DEFAULT;
    configured().unwrap_or(&DEFAULT_PROFILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_preserves_defaults_and_validates_control_families() {
        let profile = Profile::parse("\u{feff}interact = G\r\npickup=E\r\nfire=MouseX2\r\nzoom_in=F12\r\nmouse_sensitivity=2.5").unwrap();
        assert_eq!(profile.binding(Action::Interact), Key(b'G'));
        assert_eq!(profile.binding(Action::Pickup), Key(b'E'));
        assert_eq!(profile.binding(Action::Fire), Mouse(4));
        assert_eq!(profile.binding(Action::Punch), Mouse(0));
        assert_eq!(profile.binding(Action::ZoomIn), Key(0x7b));
        assert_eq!(profile.binding(Action::ZoomIn).to_string(), "F12");
        assert_eq!(profile.binding(Action::ZoomOut), WheelDown);
        assert!(profile.claims_key(b'G'));
        assert!(!profile.claims_key(b'I'));
        assert_eq!(profile.sensitivity(), Some(2.5));
        assert!(Binding::parse("Ctrl").unwrap().is_key(0xa2));
        assert!(Binding::parse("Ctrl").unwrap().is_key(0xa3));
        for text in [
            "move_forward=MouseX1",
            "aim=WheelUp",
            "aim=G\naim=H",
            "mouse_sensitivity=NaN",
            "play=K",
            "zoom_in=WheelDown",
        ] {
            assert!(Profile::parse(text).is_err(), "accepted {text}");
        }
    }
}
