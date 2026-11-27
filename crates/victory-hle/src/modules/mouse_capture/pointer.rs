//! Win32 pointer and input method for the focused game window.

use std::cell::Cell;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HWND, LRESULT, RECT};
use windows_sys::Win32::UI::{Input::Ime as ime, WindowsAndMessaging as wm};

pub(super) const TITLE_MENU_CHANGED: u32 = wm::WM_APP + 0x36B;
const SYSTEM_MENU: u8 = 1;
const SYSTEM_MOVE: u8 = 2;
static TITLE_MENU: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Default)]
struct Window {
    hwnd: usize,
    clicked: bool,
    system_ui: u8,
    active: bool,
    clip: Option<RECT>,
    original_ime: ime::HIMC,
}

thread_local! {
    // Every effect runs on the window's owner thread. Commit the state before
    // calling Win32, since IME updates can send messages back to this window.
    static WINDOW: Cell<Window> = Cell::new(Window::default());
}

pub(super) fn set_title_menu(open: bool) -> bool {
    TITLE_MENU.swap(open, Ordering::Relaxed) != open
}

fn foreground(hwnd: HWND) -> bool {
    unsafe { wm::GetForegroundWindow() == hwnd && wm::IsIconic(hwnd) == 0 }
}

fn client_rect(hwnd: HWND) -> Option<RECT> {
    let mut info = wm::WINDOWINFO {
        cbSize: size_of::<wm::WINDOWINFO>() as u32,
        ..Default::default()
    };
    if unsafe { wm::GetWindowInfo(hwnd, &mut info) } == 0 {
        return None;
    }
    let rect = info.rcClient;
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

fn same_rect(a: RECT, b: RECT) -> bool {
    (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom)
}

fn release_clip(previous: Option<RECT>) {
    if let Some(previous) = previous {
        let mut current = RECT::default();
        // The clamp is shared. Leave it alone if it has since been changed.
        unsafe {
            if wm::GetClipCursor(&mut current) != 0 && same_rect(current, previous) {
                wm::ClipCursor(std::ptr::null());
            }
        }
    }
}

fn show_if_foreground(hwnd: HWND) {
    if foreground(hwnd) {
        unsafe { wm::SetCursor(wm::LoadCursorW(null_mut(), wm::IDC_ARROW)) };
    }
}

fn release(window: &Cell<Window>, mut state: Window) {
    let hwnd = state.hwnd as HWND;
    let original_ime = std::mem::replace(&mut state.original_ime, null_mut());
    let clip = state.clip.take();
    let was_active = std::mem::replace(&mut state.active, false);
    window.set(state);
    release_clip(clip);
    if !original_ime.is_null() {
        unsafe { ime::ImmAssociateContext(hwnd, original_ime) };
    }
    if was_active {
        show_if_foreground(hwnd);
        tracing::debug!("released the gameplay cursor and input method");
    }
}

fn sync(window: &Cell<Window>, mut state: Window) {
    let hwnd = state.hwnd as HWND;
    let wanted = state.clicked && state.system_ui == 0
        && !TITLE_MENU.load(Ordering::Relaxed)
        && !super::settings_menu_open();
    if !wanted || !foreground(hwnd) {
        release(window, state);
        return;
    }
    let Some(rect) = client_rect(hwnd) else {
        release(window, state);
        return;
    };
    let newly_active = !state.active;
    state.active = true;
    window.set(state);
    if newly_active {
        let original = unsafe { ime::ImmAssociateContext(hwnd, null_mut()) };
        let mut latest = window.get();
        if latest.hwnd != state.hwnd || !latest.active {
            if !original.is_null() {
                unsafe { ime::ImmAssociateContext(hwnd, original) };
            }
            return;
        }
        latest.original_ime = original;
        window.set(latest);
        tracing::debug!("captured the gameplay cursor and detached the window input method");
    }
    let mut latest = window.get();
    if latest.clip.is_none_or(|existing| !same_rect(existing, rect))
        && unsafe { wm::ClipCursor(&rect) } != 0
    {
        latest.clip = Some(rect);
        window.set(latest);
    }
    unsafe { wm::SetCursor(null_mut()) };
}

pub(super) fn active(hwnd: HWND) -> bool {
    WINDOW.with(|window| {
        let state = window.get();
        state.hwnd == hwnd as usize && state.active
            && !TITLE_MENU.load(Ordering::Relaxed) && !super::settings_menu_open()
    })
}

pub(super) fn click(hwnd: HWND) -> bool {
    if TITLE_MENU.load(Ordering::Relaxed) || super::settings_menu_open() || !foreground(hwnd) {
        return false;
    }
    WINDOW.with(|window| {
        let mut state = window.get();
        if state.hwnd != hwnd as usize {
            release(window, state);
            state = Window { hwnd: hwnd as usize, ..Window::default() };
        }
        if state.active || state.system_ui != 0 {
            return false;
        }
        state.clicked = true;
        sync(window, state);
        active(hwnd)
    })
}

pub(super) fn message(
    hwnd: HWND,
    msg: u32,
    w: usize,
    l: isize,
    tracked: bool,
) -> Option<LRESULT> {
    WINDOW.with(|window| {
        let mut state = window.get();
        if state.hwnd != hwnd as usize {
            return None;
        }
        match msg {
            wm::WM_SETCURSOR if state.active && tracked && foreground(hwnd) => {
                if l as u16 == wm::HTCLIENT as u16 && !TITLE_MENU.load(Ordering::Relaxed) {
                    unsafe { wm::SetCursor(null_mut()) };
                    return Some(1);
                }
            }
            wm::WM_IME_STARTCOMPOSITION | wm::WM_IME_COMPOSITION | wm::WM_IME_CHAR
                if state.active =>
            {
                return Some(0);
            }
            wm::WM_KILLFOCUS | wm::WM_CANCELMODE | wm::WM_NCDESTROY | wm::WM_CLOSE => {
                state.clicked = false;
                state.system_ui = 0;
                release(window, state);
            }
            wm::WM_ACTIVATEAPP if w == 0 => {
                state.clicked = false;
                state.system_ui = 0;
                release(window, state);
            }
            wm::WM_ACTIVATE if w as u16 == wm::WA_INACTIVE as u16 => {
                state.clicked = false;
                release(window, state);
            }
            wm::WM_SYSCOMMAND if w & 0xfff0 == wm::SC_CLOSE as usize => {
                state.clicked = false;
                release(window, state);
            }
            wm::WM_ENTERMENULOOP | wm::WM_ENTERSIZEMOVE => {
                state.system_ui |= if msg == wm::WM_ENTERMENULOOP { SYSTEM_MENU } else { SYSTEM_MOVE };
                release(window, state);
            }
            wm::WM_EXITMENULOOP | wm::WM_EXITSIZEMOVE => {
                state.system_ui &= !(if msg == wm::WM_EXITMENULOOP { SYSTEM_MENU } else { SYSTEM_MOVE });
                sync(window, state);
            }
            wm::WM_WINDOWPOSCHANGED | TITLE_MENU_CHANGED if tracked => sync(window, state),
            _ => {}
        }
        None
    })
}
