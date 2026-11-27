//! Foreground, relative mouse input for the title window. Native readings are
//! cumulative. The optional look adapter also serves titles that request only a pad.
mod gameplay;
mod pointer;

use super::gameinput::MouseState;
use gameplay::Gameplay;
pub use gameplay::{MouseGameplayInput, MouseGameplayMode, MouseWheelStep};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use victory_loader::input::{ActiveInput, note_input};
use victory_loader::input::profile::{self, Binding};
use windows_sys::Win32::Foundation::{HWND, LRESULT, POINT};
use windows_sys::Win32::UI::{Input as raw, WindowsAndMessaging as wm};

static CAPTURE: LazyLock<Mutex<Capture>> = LazyLock::new(Default::default);
static LOOK: LazyLock<bool> = LazyLock::new(|| enabled("VICTORY_MOUSE_LOOK"));
static DIAG: LazyLock<bool> = LazyLock::new(|| enabled("VICTORY_MOUSE_DIAG"));
static TRACE_ACTIONS: LazyLock<bool> = LazyLock::new(|| enabled("VICTORY_MOUSE_ACTIONS_TRACE"));
static TEST: LazyLock<bool> = LazyLock::new(|| enabled("VICTORY_MOUSE_TEST"));
static GAIN: LazyLock<f32> = LazyLock::new(|| {
    if let Some(gain) = profile::current().sensitivity() {
        return gain;
    }
    std::env::var("VICTORY_MOUSE_SENSITIVITY")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|n| n.is_finite() && *n > 0.0)
        .unwrap_or(1.0)
        .clamp(0.05, 10.0)
});
const TEST_DELTA: u32 = wm::WM_APP + 0x36A;
const IN_SYSTEM_MENU: u8 = 1;
const IN_SYSTEM_MOVE: u8 = 2;
// RAWMOUSE button reporting consists of five down/up bit pairs.
const ACTION_FLAGS: u32 = 0x3ff | wm::RI_MOUSE_WHEEL;
fn enabled(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|v| v == "1")
}

pub(super) fn uses_relative_look() -> bool {
    *LOOK
}

fn settings_menu_open() -> bool {
    victory_gpu::SETTINGS_MENU_OPEN.load(Ordering::Relaxed)
}

pub(super) fn settings_menu_changed() {
    let hwnd = {
        let mut capture = locked();
        capture.native.buttons = 0;
        capture.look = Look::default();
        capture.gameplay.set_mode(MouseGameplayMode::Inactive);
        capture.keyboard_modifiers = [0; 3];
        capture.hwnd as HWND
    };
    if !hwnd.is_null() {
        // SAFETY: synchronize the cursor and IME on the owning window thread.
        unsafe { wm::PostMessageW(hwnd, pointer::TITLE_MENU_CHANGED, 0, 0) };
    }
}

#[derive(Default)]
struct Capture {
    hwnd: usize,
    native: MouseState,
    absolute: HashMap<usize, (i32, i32)>,
    look: Look,
    gameplay: Gameplay,
    keyboard_modifiers: [u8; 3],
    system_ui: u8,
}

#[derive(Default)]
struct Look {
    samples: VecDeque<(Instant, i64, i64)>,
    last_read: Option<Instant>,
    period_ms: Option<f32>,
}

impl Look {
    fn read(&mut self, now: Instant) {
        if let Some(last) = self.last_read {
            let ms = now.duration_since(last).as_secs_f32() * 1000.0;
            if ms < 4.0 {
                return;
            }
            if ms <= 200.0 {
                let prev = self.period_ms.unwrap_or(ms);
                self.period_ms = Some(prev * 0.75 + ms * 0.25);
            }
        }
        self.last_read = Some(now);
    }

    fn window(&self) -> Duration {
        Duration::from_secs_f32((self.period_ms.unwrap_or(32.0) * 1.25).clamp(24.0, 100.0) / 1000.0)
    }

    fn trim(&mut self, now: Instant) {
        let span = self.window();
        while self
            .samples
            .front()
            .is_some_and(|s| now.duration_since(s.0) >= span)
        {
            self.samples.pop_front();
        }
    }

    fn motion(&mut self, now: Instant, x: i32, y: i32) {
        self.trim(now);
        // Coalesce high polling-rate mice into one bin per millisecond.
        if let Some(last) = self.samples.back_mut()
            && now.duration_since(last.0) < Duration::from_millis(1)
        {
            last.1 += i64::from(x);
            last.2 += i64::from(y);
        } else {
            self.samples.push_back((now, x.into(), y.into()));
        }
    }

    fn axes(&mut self, now: Instant, gain: f32) -> Option<(f32, f32)> {
        self.trim(now);
        let (x, y) = self
            .samples
            .iter()
            .fold((0i64, 0i64), |(x, y), s| (x + s.1, y + s.2));
        if x == 0 && y == 0 {
            return None;
        }
        let (x, y) = (x as f32, -(y as f32));
        let len = x.hypot(y);
        // A console stick expresses a turning rate. Normalize counts by the
        // sampling interval and compensate the title's central dead zone.
        let speed = (len * gain / (self.window().as_secs_f32() * 1600.0)).min(1.0);
        let scale = (0.24 + 0.76 * speed) / len;
        Some((x * scale, y * scale))
    }
}

fn locked() -> MutexGuard<'static, Capture> {
    CAPTURE.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn state() -> Option<MouseState> {
    let c = locked();
    (c.hwnd != 0).then_some(c.native)
}

pub(crate) fn observe_pad_read() {
    if *LOOK {
        locked().look.read(Instant::now());
    }
}

pub(crate) fn camera_stick() -> Option<(f32, f32)> {
    if !*LOOK || settings_menu_open() {
        return None;
    }
    locked().look.axes(Instant::now(), *GAIN)
}

pub fn mouse_sensitivity() -> f32 {
    *GAIN
}

pub fn set_title_pointer_menu(open: bool) {
    if *LOOK && pointer::set_title_menu(open) {
        let hwnd = {
            let mut capture = locked();
            if open {
                capture.look = Look::default();
            }
            capture.hwnd as HWND
        };
        if !hwnd.is_null() {
            unsafe { wm::PostMessageW(hwnd, pointer::TITLE_MENU_CHANGED, 0, 0) };
        }
    }
}

/// Called once per update of the title's active gameplay input context.
/// The window has to stay foreground for any button or wheel step to survive.
pub fn sample_mouse_gameplay(mode: MouseGameplayMode) -> MouseGameplayInput {
    let mut capture = locked();
    let foreground = unsafe { wm::GetForegroundWindow() } as usize;
    let mode = if capture.hwnd != 0 && capture.hwnd == foreground && capture.system_ui == 0
        && !settings_menu_open()
    {
        mode
    } else {
        MouseGameplayMode::Inactive
    };
    if *TRACE_ACTIONS && capture.gameplay.mode != mode {
        tracing::info!(before = ?capture.gameplay.mode, after = ?mode, "title mouse input ownership changed");
    }
    capture.gameplay.set_mode(mode);
    capture.gameplay.sample()
}

/// Check the title window again before a gameplay read consumes a latched edge.
pub fn mouse_gameplay_has_focus() -> bool {
    let capture = locked();
    capture.gameplay.mode != MouseGameplayMode::Inactive
        && !settings_menu_open()
        && capture.system_ui == 0
        && capture.hwnd != 0
        && capture.hwnd == unsafe { wm::GetForegroundWindow() } as usize
}

pub(super) fn gameplay_key(window: usize, vk: u8, down: bool) {
    let Some(profile) = profile::configured() else { return };
    let mut capture = locked();
    if capture.hwnd == window && capture.system_ui == 0
        && let Some(vk) = capture.logical_key(vk, down)
    {
        capture.gameplay.digital(Binding::Key(vk), down, true, profile);
    }
}

pub(super) fn register(hwnd: HWND) {
    if unsafe { wm::GetAncestor(hwnd, wm::GA_ROOT) } != hwnd {
        return;
    }
    let mut c = locked();
    if c.hwnd == hwnd as usize {
        return;
    }
    if c.hwnd != 0 && unsafe { wm::GetForegroundWindow() } != hwnd {
        return;
    }
    let device = raw::RAWINPUTDEVICE {
        usUsagePage: 1,
        usUsage: 2,
        dwFlags: 0,
        hwndTarget: hwnd,
    };
    if unsafe { raw::RegisterRawInputDevices(&device, 1, size_of_val(&device) as u32) } == 0 {
        tracing::warn!(error = %std::io::Error::last_os_error(), "could not register title raw mouse");
        return;
    }
    c.hwnd = hwnd as usize;
    c.reset_motion();
    tracing::info!(hwnd = c.hwnd, look = *LOOK, "title raw mouse registered");
}

impl Capture {
    /// Generic Shift/Ctrl/Alt bindings remain held until both sides release.
    fn logical_key(&mut self, vk: u8, down: bool) -> Option<u8> {
        if !(0xa0..=0xa5).contains(&vk) {
            return Some(vk);
        }
        let index = (vk - 0xa0) / 2;
        let sides = &mut self.keyboard_modifiers[index as usize];
        let was_down = *sides != 0;
        let side = 1 << (vk & 1);
        if down { *sides |= side; } else { *sides &= !side; }
        (was_down != (*sides != 0)).then_some(0x10 + index)
    }

    fn reset_motion(&mut self) {
        self.native.buttons = 0;
        self.absolute.clear();
        self.look = Look::default();
        self.gameplay = Gameplay::default();
        self.keyboard_modifiers = [0; 3];
        self.system_ui = 0;
    }

    fn gameplay_events(&mut self, flags: u16, data: i16, client: bool) {
        let flags = u32::from(flags);
        let profile = profile::current();
        for button in 0..5 {
            let (down, up) = (1 << (2 * button), 2 << (2 * button));
            for (flag, pressed) in [(down, true), (up, false)] {
                if flags & flag != 0 {
                    let accepted = self.gameplay.digital(Binding::Mouse(button), pressed, client, profile);
                    if *TRACE_ACTIONS {
                        tracing::info!(?button, pressed, client, accepted, mode = ?self.gameplay.mode, "raw title mouse button");
                    }
                }
            }
        }
        if flags & wm::RI_MOUSE_WHEEL != 0 {
            let accepted = self.gameplay.wheel(data, client);
            if *TRACE_ACTIONS {
                tracing::info!(delta = data, client, accepted, mode = ?self.gameplay.mode, "raw title mouse wheel");
            }
        }
    }

    fn packet(&mut self, dx: i32, dy: i32, flags: u16, data: i16, mouse_look: bool) {
        self.native.position_x = self.native.position_x.saturating_add(dx.into());
        self.native.position_y = self.native.position_y.saturating_add(dy.into());
        for i in 0..5 {
            if flags & (1 << (i * 2)) != 0 {
                self.native.buttons |= 1 << i;
            }
            if flags & (2 << (i * 2)) != 0 {
                self.native.buttons &= !(1 << i);
            }
        }
        if flags & 0x400 != 0 {
            self.native.wheel_y += i64::from(data);
        }
        if flags & 0x800 != 0 {
            self.native.wheel_x += i64::from(data);
        }
        if mouse_look && (dx != 0 || dy != 0) {
            self.look.motion(Instant::now(), dx, dy);
        }
        if dx != 0 || dy != 0 || flags != 0 {
            note_input(ActiveInput::Keyboard);
        }
    }
}

pub(super) fn pointer_message(hwnd: HWND, msg: u32, w: usize, l: isize) -> Option<LRESULT> {
    if !matches!(
        msg,
        wm::WM_SETCURSOR | wm::WM_IME_STARTCOMPOSITION | wm::WM_IME_COMPOSITION
            | wm::WM_IME_CHAR | wm::WM_KILLFOCUS | wm::WM_CANCELMODE
            | wm::WM_NCDESTROY | wm::WM_CLOSE | wm::WM_ACTIVATE | wm::WM_ACTIVATEAPP | wm::WM_SYSCOMMAND
            | wm::WM_ENTERMENULOOP | wm::WM_EXITMENULOOP | wm::WM_ENTERSIZEMOVE
            | wm::WM_EXITSIZEMOVE | wm::WM_WINDOWPOSCHANGED | pointer::TITLE_MENU_CHANGED
    ) || !*LOOK {
        return None;
    }
    let tracked = locked().hwnd == hwnd as usize;
    pointer::message(hwnd, msg, w, l, tracked)
}

pub(super) fn message(hwnd: HWND, msg: u32, w: usize, l: isize) {
    if msg == wm::WM_SETFOCUS {
        register(hwnd);
        return;
    }
    if msg == wm::WM_INPUT {
        raw_mouse(hwnd, w, l);
        return;
    }
    if !matches!(
        msg,
        wm::WM_KILLFOCUS
            | wm::WM_ACTIVATEAPP
            | wm::WM_NCDESTROY
            | wm::WM_MOUSEACTIVATE
            | wm::WM_CANCELMODE
            | wm::WM_ENTERMENULOOP
            | wm::WM_EXITMENULOOP
            | wm::WM_ENTERSIZEMOVE
            | wm::WM_EXITSIZEMOVE
            | TEST_DELTA
    ) {
        return;
    }
    let mut c = locked();
    if c.hwnd != hwnd as usize {
        return;
    }
    match msg {
        wm::WM_NCDESTROY => {
            c.reset_motion();
            c.hwnd = 0;
        }
        wm::WM_KILLFOCUS => c.reset_motion(),
        wm::WM_ACTIVATEAPP if w == 0 => c.reset_motion(),
        wm::WM_MOUSEACTIVATE => {
            let button = match l as u32 >> 16 {
                wm::WM_LBUTTONDOWN | wm::WM_LBUTTONDBLCLK => Some(0),
                wm::WM_RBUTTONDOWN | wm::WM_RBUTTONDBLCLK => Some(1),
                wm::WM_MBUTTONDOWN | wm::WM_MBUTTONDBLCLK => Some(2),
                // The activation message omits which X button was used.
                wm::WM_XBUTTONDOWN | wm::WM_XBUTTONDBLCLK => {
                    c.gameplay.block_activation_x();
                    None
                }
                _ => None,
            };
            if let Some(button) = button {
                c.gameplay.block_activation_click(button);
            }
        }
        wm::WM_CANCELMODE => c.gameplay.set_mode(MouseGameplayMode::Inactive),
        wm::WM_ENTERMENULOOP => {
            c.system_ui |= IN_SYSTEM_MENU;
            c.gameplay.set_mode(MouseGameplayMode::Inactive);
        }
        wm::WM_EXITMENULOOP => c.system_ui &= !IN_SYSTEM_MENU,
        wm::WM_ENTERSIZEMOVE => {
            c.system_ui |= IN_SYSTEM_MOVE;
            c.gameplay.set_mode(MouseGameplayMode::Inactive);
        }
        wm::WM_EXITSIZEMOVE => c.system_ui &= !IN_SYSTEM_MOVE,
        TEST_DELTA if *TEST => {
            let (dx, dy) = (w as i32, l as i32);
            if dx.unsigned_abs() <= 4096 && dy.unsigned_abs() <= 4096 {
                c.packet(dx, dy, 0, 0, *LOOK);
                if *DIAG {
                    tracing::info!(
                        dx,
                        dy,
                        x = c.native.position_x,
                        y = c.native.position_y,
                        "test title mouse delta"
                    );
                }
            }
        }
        _ => {}
    }
}

fn cursor_in_client(hwnd: HWND) -> bool {
    let mut point = POINT::default();
    let mut info = wm::WINDOWINFO {
        cbSize: size_of::<wm::WINDOWINFO>() as u32,
        ..Default::default()
    };
    // WindowFromPoint may call a window procedure. The caller does not hold
    // the capture lock here, and our window always keeps its ordinary messages.
    unsafe {
        if wm::GetCursorPos(&mut point) == 0 || wm::GetWindowInfo(hwnd, &mut info) == 0 {
            return false;
        }
        let rect = info.rcClient;
        if !(rect.left..rect.right).contains(&point.x)
            || !(rect.top..rect.bottom).contains(&point.y)
        {
            return false;
        }
        let pointed = wm::WindowFromPoint(point);
        pointed == hwnd || (!pointed.is_null() && wm::GetAncestor(pointed, wm::GA_ROOT) == hwnd)
    }
}

fn raw_mouse(hwnd: HWND, w: usize, l: isize) {
    if w != 0 || settings_menu_open() || unsafe { wm::GetForegroundWindow() } != hwnd {
        return;
    }
    let mut packet = raw::RAWINPUT::default();
    let mut size = size_of_val(&packet) as u32;
    let got = unsafe {
        raw::GetRawInputData(
            l as _,
            raw::RID_INPUT,
            (&raw mut packet).cast(),
            &mut size,
            size_of::<raw::RAWINPUTHEADER>() as u32,
        )
    };
    let needed = (size_of::<raw::RAWINPUTHEADER>() + size_of::<raw::RAWMOUSE>()) as u32;
    if got == u32::MAX || got < needed || packet.header.dwType != raw::RIM_TYPEMOUSE {
        return;
    }
    let m = unsafe { packet.data.mouse };
    let buttons = unsafe { m.Anonymous.Anonymous };
    let flags = buttons.usButtonFlags;
    let data = buttons.usButtonData as i16;
    let actions = u32::from(flags) & ACTION_FLAGS != 0;
    let mut client = actions && cursor_in_client(hwnd);
    let taking_control = if *LOOK && client && flags & wm::RI_MOUSE_LEFT_BUTTON_DOWN as u16 != 0 {
        let tracked = locked().hwnd == hwnd as usize;
        tracked && pointer::click(hwnd)
    } else {
        false
    };
    if taking_control {
        // The click that takes control belongs to the window, not the game.
        client = false;
    }
    let captured = *LOOK && pointer::active(hwnd);
    let mut capture = locked();
    if capture.hwnd != hwnd as usize || unsafe { wm::GetForegroundWindow() } != hwnd {
        return;
    }
    let (dx, dy) = if m.usFlags & raw::MOUSE_MOVE_ABSOLUTE == 0 {
        (m.lLastX, m.lLastY)
    } else {
        let virtual_desktop = m.usFlags & raw::MOUSE_VIRTUAL_DESKTOP != 0;
        let (sx, sy) = if virtual_desktop {
            (wm::SM_CXVIRTUALSCREEN, wm::SM_CYVIRTUALSCREEN)
        } else {
            (wm::SM_CXSCREEN, wm::SM_CYSCREEN)
        };
        let pixel = |v: i32, axis| {
            ((v.clamp(0, 65535) as i64
                * i64::from(unsafe { wm::GetSystemMetrics(axis) }.saturating_sub(1)))
                / 65535) as i32
        };
        let pos = (pixel(m.lLastX, sx), pixel(m.lLastY, sy));
        let old = capture
            .absolute
            .insert(packet.header.hDevice as usize, pos)
            .unwrap_or(pos);
        (pos.0 - old.0, pos.1 - old.1)
    };
    let native_flags = if taking_control { flags & !(wm::RI_MOUSE_LEFT_BUTTON_DOWN as u16) } else { flags };
    capture.packet(dx, dy, native_flags, data, captured);
    if actions {
        capture.gameplay_events(flags, data, client && (!*LOOK || captured));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relative_motion_keeps_direction_and_stops() {
        let start = Instant::now();
        let mut s = Look::default();
        s.motion(start, 20, -10);
        let (x, y) = s.axes(start, 1.0).unwrap();
        assert!(x > 0.0 && y > 0.0 && (x / y - 2.0).abs() < 0.0001);
        s.motion(start, -20, 10);
        assert_eq!(s.axes(start, 1.0), None);
        s.motion(start, -600, 800);
        let (x, y) = s.axes(start, 1.0).unwrap();
        assert!((x + 0.6).abs() < 0.0001 && (y + 0.8).abs() < 0.0001);
        assert_eq!(s.axes(start + Duration::from_millis(101), 1.0), None);
    }

    #[test]
    fn modifier_stays_held_while_either_side_is_down() {
        let mut capture = Capture::default();
        assert_eq!(capture.logical_key(0xa2, true), Some(0x11));
        assert_eq!(capture.logical_key(0xa3, true), None);
        assert_eq!(capture.logical_key(0xa2, false), None);
        assert_eq!(capture.logical_key(0xa3, false), Some(0x11));
        assert_eq!(capture.logical_key(0xa0, true), Some(0x10));
        capture.reset_motion();
        assert_eq!(capture.logical_key(0xa1, true), Some(0x10));
    }

    #[test]
    fn focus_reset_preserves_native_coordinates_and_releases_buttons() {
        let mut c = Capture::default();
        c.gameplay.set_mode(MouseGameplayMode::Scope);
        c.gameplay_events(
            (wm::RI_MOUSE_LEFT_BUTTON_DOWN | wm::RI_MOUSE_WHEEL) as u16,
            240,
            true,
        );
        assert!(c.gameplay.sample().fire);
        c.packet(35, -12, 1 | 4 | 0x400, -120, false);
        assert_eq!(
            (c.native.position_x, c.native.position_y, c.native.wheel_y),
            (35, -12, -120)
        );
        assert_eq!(c.native.buttons, 3);
        c.reset_motion();
        assert_eq!(c.gameplay.mode, MouseGameplayMode::Inactive);
        assert_eq!(c.gameplay.sample(), MouseGameplayInput::default());
        assert_eq!(
            (
                c.native.position_x,
                c.native.position_y,
                c.native.wheel_y,
                c.native.buttons
            ),
            (35, -12, -120, 0)
        );
    }

    #[test]
    fn native_mouse_keeps_cumulative_raw_wheel_while_gameplay_uses_detents() {
        let mut c = Capture::default();
        c.gameplay.set_mode(MouseGameplayMode::WeaponWheel);
        let vertical = wm::RI_MOUSE_WHEEL as u16;
        let horizontal = wm::RI_MOUSE_HWHEEL as u16;
        for (flags, delta) in [(vertical, 30), (horizontal, 240), (vertical, 210)] {
            c.packet(0, 0, flags, delta, false);
            c.gameplay_events(flags, delta, true);
        }
        assert_eq!((c.native.wheel_x, c.native.wheel_y), (240, 240));
        assert_eq!(c.gameplay.sample().wheel, Some(MouseWheelStep::Up));
        assert_eq!(c.gameplay.sample().wheel, None);
        assert_eq!(c.gameplay.sample().wheel, Some(MouseWheelStep::Up));
        assert_eq!((c.native.wheel_x, c.native.wheel_y), (240, 240));
    }
}
