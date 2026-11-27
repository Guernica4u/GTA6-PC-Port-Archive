//! Per-title-sample mouse buttons and wheel edges. Raw GameInput readings keep
//! their own cumulative wheel state.

use std::collections::VecDeque;
use victory_loader::input::profile::{self, Action, Binding, Profile};

const WHEEL_DELTA: i32 = 120;
// A single signed RAWMOUSE wheel report can contain up to 274 detents.
// Bound a burst across many packets while the title is stalled.
const MAX_PENDING_WHEEL_STEPS: usize = 512;

/// The currently active native input destination, sampled by the title proxy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MouseGameplayMode {
    #[default]
    Inactive,
    OnFoot,
    Scope,
    WeaponWheel,
}

impl MouseGameplayMode {
    fn accepts_buttons(self) -> bool {
        matches!(self, Self::OnFoot | Self::Scope)
    }

    fn accepts_wheel(self) -> bool {
        matches!(self, Self::Scope | Self::WeaponWheel)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseWheelStep {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MouseGameplayInput {
    pub aim: bool,
    pub fire: bool,
    pub punch: bool,
    pub wheel: Option<MouseWheelStep>,
}

const CONTINUOUS: [Action; 3] = [Action::Aim, Action::Fire, Action::Punch];

#[derive(Default)]
pub(super) struct Gameplay {
    pub(super) mode: MouseGameplayMode,
    down: u8,
    fresh: u8,
    activation_click: u8,
    activation_extra: bool,
    wheel: Wheel,
}

impl Gameplay {
    pub(super) fn set_mode(&mut self, mode: MouseGameplayMode) {
        if self.mode == mode {
            return;
        }
        if !mode.accepts_buttons() {
            self.down = 0;
            self.fresh = 0;
        }
        self.wheel.clear();
        self.mode = mode;
    }

    // The click which makes a window active belongs to window management.
    pub(super) fn block_activation_click(&mut self, button: u8) {
        self.activation_click |= 1 << button;
        let mask = Self::continuous_mask(Binding::Mouse(button), profile::current());
        self.fresh &= !mask;
        self.down &= !mask;
    }

    pub(super) fn block_activation_x(&mut self) {
        self.activation_extra = true;
        let profile = profile::current();
        let mask = Self::continuous_mask(Binding::Mouse(3), profile)
            | Self::continuous_mask(Binding::Mouse(4), profile);
        self.fresh &= !mask;
        self.down &= !mask;
    }

    fn continuous_mask(binding: Binding, profile: &Profile) -> u8 {
        CONTINUOUS
            .into_iter()
            .enumerate()
            .fold(0, |mask, (bit, action)| {
                mask | (u8::from(profile.binding(action) == binding) << bit)
            })
    }

    fn release(&mut self, binding: Binding, profile: &Profile) -> bool {
        let mask = Self::continuous_mask(binding, profile);
        let accepted = self.down & mask != 0;
        self.down &= !mask;
        accepted
    }

    fn scroll_direction(
        mode: MouseGameplayMode,
        binding: Binding,
        profile: &Profile,
    ) -> Option<MouseWheelStep> {
        let (up, down) = match mode {
            MouseGameplayMode::Scope => (Action::ZoomIn, Action::ZoomOut),
            MouseGameplayMode::WeaponWheel => (Action::WheelNext, Action::WheelPrevious),
            _ => return None,
        };
        if profile.binding(up) == binding {
            Some(MouseWheelStep::Up)
        } else if profile.binding(down) == binding {
            Some(MouseWheelStep::Down)
        } else {
            None
        }
    }

    pub(super) fn digital(
        &mut self,
        binding: Binding,
        pressed: bool,
        client: bool,
        profile: &Profile,
    ) -> bool {
        if let Binding::Mouse(button) = binding {
            let bit = 1 << button;
            if button >= 3 && std::mem::take(&mut self.activation_extra) && pressed {
                self.activation_click |= bit;
            }
            if !pressed {
                self.activation_click &= !bit;
            } else if self.activation_click & bit != 0 {
                return false;
            }
        }
        if !pressed {
            return self.release(binding, profile);
        }
        if !client {
            return false;
        }
        let mask = if self.mode.accepts_buttons() {
            Self::continuous_mask(binding, profile)
        } else {
            0
        };
        self.fresh |= mask & !self.down;
        self.down |= mask;
        let step = Self::scroll_direction(self.mode, binding, profile);
        if let Some(step) = step {
            self.wheel.push_step(step, 1);
        }
        mask != 0 || step.is_some()
    }

    pub(super) fn wheel(&mut self, delta: i16, client: bool) -> bool {
        if client && self.mode.accepts_wheel() {
            let mode = self.mode;
            self.wheel.push(delta, |binding| {
                Self::scroll_direction(mode, binding, profile::current())
            });
            true
        } else {
            false
        }
    }

    pub(super) fn sample(&mut self) -> MouseGameplayInput {
        let buttons = self.down | std::mem::take(&mut self.fresh);
        MouseGameplayInput {
            aim: buttons & 1 != 0,
            fire: buttons & 2 != 0,
            punch: buttons & 4 != 0,
            wheel: self.wheel.next(),
        }
    }
}

#[derive(Default)]
struct Wheel {
    remainder: i32,
    pending: VecDeque<MouseWheelStep>,
    release_due: bool,
}

impl Wheel {
    fn push(&mut self, delta: i16, direction: impl FnOnce(Binding) -> Option<MouseWheelStep>) {
        let distance = self.remainder + i32::from(delta);
        let steps = distance / WHEEL_DELTA;
        self.remainder = distance % WHEEL_DELTA;
        if steps != 0 {
            let binding = if steps > 0 {
                Binding::WheelUp
            } else {
                Binding::WheelDown
            };
            if let Some(step) = direction(binding) {
                self.push_step(step, steps.unsigned_abs() as usize);
            }
        }
    }

    fn push_step(&mut self, step: MouseWheelStep, count: usize) {
        self.pending.extend(std::iter::repeat_n(
            step,
            count.min(MAX_PENDING_WHEEL_STEPS - self.pending.len()),
        ));
    }

    fn next(&mut self) -> Option<MouseWheelStep> {
        if std::mem::take(&mut self.release_due) {
            return None;
        }
        let step = self.pending.pop_front();
        self.release_due = step.is_some();
        step
    }

    fn clear(&mut self) {
        self.remainder = 0;
        self.pending.clear();
        self.release_due = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(state: &mut Gameplay, index: u8, pressed: bool, client: bool) -> bool {
        state.digital(Binding::Mouse(index), pressed, client, profile::current())
    }

    #[test]
    fn gameplay_mouse_requires_an_active_client_press_and_releases_on_exit() {
        let mut state = Gameplay::default();
        assert!(!button(&mut state, 0, true, true));
        state.set_mode(MouseGameplayMode::OnFoot);
        assert_eq!(state.sample(), MouseGameplayInput::default());
        assert!(!button(&mut state, 0, true, false));
        assert!(button(&mut state, 1, true, true));
        assert!(state.sample().aim);
        state.set_mode(MouseGameplayMode::Scope);
        assert!(state.sample().aim);
        assert!(button(&mut state, 1, false, false));
        assert!(!state.sample().aim);
        assert!(button(&mut state, 0, true, true));
        state.set_mode(MouseGameplayMode::Inactive);
        state.set_mode(MouseGameplayMode::OnFoot);
        assert_eq!(state.sample(), MouseGameplayInput::default());
    }

    #[test]
    fn gameplay_mouse_keeps_a_short_click_but_rejects_the_activation_click() {
        let mut state = Gameplay::default();
        state.block_activation_click(0);
        state.set_mode(MouseGameplayMode::OnFoot);
        assert!(!button(&mut state, 0, true, true));
        assert!(!state.sample().fire);
        assert!(!button(&mut state, 0, false, true));
        assert!(button(&mut state, 0, true, true));
        assert!(button(&mut state, 0, false, true));
        assert!(state.sample().fire);
        assert!(!state.sample().fire);
    }

    #[test]
    fn rebound_digital_actions_and_scope_steps_keep_their_own_input() {
        let profile =
            Profile::parse("fire=MouseX1\npunch=G\naim=H\nzoom_in=J\nwheel_next=MouseMiddle")
                .unwrap();
        let mut state = Gameplay::default();
        state.set_mode(MouseGameplayMode::OnFoot);
        assert!(!state.digital(Binding::Mouse(0), true, true, &profile));
        assert!(state.digital(Binding::Key(b'G'), true, true, &profile));
        assert!(state.digital(Binding::Key(b'G'), false, true, &profile));
        assert_eq!(
            state.sample(),
            MouseGameplayInput {
                punch: true,
                ..Default::default()
            }
        );
        assert_eq!(state.sample(), MouseGameplayInput::default());
        assert!(state.digital(Binding::Mouse(3), true, true, &profile));
        assert!(state.sample().fire);
        state.set_mode(MouseGameplayMode::Scope);
        assert!(state.digital(Binding::Key(b'J'), true, true, &profile));
        assert_eq!(state.sample().wheel, Some(MouseWheelStep::Up));
        state.set_mode(MouseGameplayMode::WeaponWheel);
        assert!(!state.digital(Binding::Key(b'J'), true, true, &profile));
        assert!(state.digital(Binding::Mouse(2), true, true, &profile));
        assert_eq!(state.sample().wheel, Some(MouseWheelStep::Up));
        assert!(!state.sample().fire);
    }

    #[test]
    fn wheel_keeps_high_precision_detents_order_and_release_samples() {
        let mut state = Gameplay::default();
        state.set_mode(MouseGameplayMode::Scope);
        assert!(state.wheel(50, true));
        assert_eq!(state.sample().wheel, None);
        assert!(state.wheel(190, true));
        assert!(state.wheel(-360, true));
        use MouseWheelStep::{Down, Up};
        let samples: Vec<_> = (0..10).map(|_| state.sample().wheel).collect();
        assert_eq!(
            samples,
            [
                Some(Up),
                None,
                Some(Up),
                None,
                Some(Down),
                None,
                Some(Down),
                None,
                Some(Down),
                None
            ]
        );
        assert_eq!(state.sample().wheel, None);
    }

    #[test]
    fn wheel_does_not_carry_partial_or_pending_steps_between_native_contexts() {
        let mut state = Gameplay::default();
        state.set_mode(MouseGameplayMode::OnFoot);
        assert!(!state.wheel(240, true));
        state.set_mode(MouseGameplayMode::Scope);
        assert!(!state.wheel(120, false));
        assert!(state.wheel(310, true));
        assert_eq!(state.sample().wheel, Some(MouseWheelStep::Up));
        state.set_mode(MouseGameplayMode::WeaponWheel);
        assert_eq!(state.sample(), MouseGameplayInput::default());
        assert!(!button(&mut state, 0, true, true));
        assert!(state.wheel(70, true));
        assert!(state.wheel(-20, true));
        assert_eq!(state.sample().wheel, None);
        assert!(state.wheel(70, true));
        assert_eq!(state.sample().wheel, Some(MouseWheelStep::Up));
    }
}
