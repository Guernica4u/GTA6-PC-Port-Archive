//! The title's life cycle. A console title is never told it is closing:
//! quitting it or launching a new game first constrains it (its
//! constrained-change routines run), then suspends it (its suspend routines
//! run, and it has a second to save) and then ends the process. Closing the
//! title's window does the same here; a new launch then starts `victory`
//! again, which waits for this process to be gone.
#![allow(non_snake_case)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::too_many_arguments)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
use windows_sys::Win32::UI::Input::KeyboardAndMouse as keyboard;
use windows_sys::Win32::UI::WindowsAndMessaging::{self as wm, HMENU};
use windows_sys::core::{PCSTR, PCWSTR, w};

use crate::{HleModule, hle_export};

/// A suspend routine (`quiesced`) or a constrained-change routine
/// (`constrained`): both take a flag and the title's context.
type Routine = extern "system" fn(flag: u8, context: *mut c_void);

/// Registration id, routine and context.
type Registration = (usize, Routine, usize);

/// Each suspend routine.
static ROUTINES: Mutex<Vec<Registration>> = Mutex::new(Vec::new());

/// Each constrained-change routine.
static CONSTRAINED: Mutex<Vec<Registration>> = Mutex::new(Vec::new());

fn routines() -> MutexGuard<'static, Vec<Registration>> {
    ROUTINES.lock().unwrap_or_else(|e| e.into_inner())
}

fn constrained() -> MutexGuard<'static, Vec<Registration>> {
    CONSTRAINED.lock().unwrap_or_else(|e| e.into_inner())
}

/// How long the console lets suspend routines run before it ends the title.
const SUSPEND_DEADLINE: Duration = Duration::from_secs(1);

const ERROR_INVALID_PARAMETER: u32 = 87;

/// Adds `routine` to `list` and writes its registration id.
fn register(
    mut list: MutexGuard<'static, Vec<Registration>>,
    routine: Option<Routine>,
    context: *mut c_void,
    registration: *mut usize,
) -> u32 {
    let Some(routine) = routine else {
        return ERROR_INVALID_PARAMETER;
    };
    if registration.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    list.push((id, routine, context as usize));
    // SAFETY: checked non-null; the title passes room for its registration.
    unsafe { registration.write(id) };
    0
}

pub extern "system" fn RegisterAppStateChangeNotification(
    routine: Option<Routine>,
    context: *mut c_void,
    registration: *mut usize,
) -> u32 {
    register(routines(), routine, context, registration)
}

pub extern "system" fn UnregisterAppStateChangeNotification(registration: usize) {
    routines().retain(|&(id, ..)| id != registration);
}

/// The host's own version never calls the routine (a desktop app is never
/// constrained), so a title waiting for it would never suspend.
pub extern "system" fn RegisterAppConstrainedChangeNotification(
    routine: Option<Routine>,
    context: *mut c_void,
    registration: *mut usize,
) -> u32 {
    register(constrained(), routine, context, registration)
}

pub extern "system" fn UnregisterAppConstrainedChangeNotification(registration: usize) {
    constrained().retain(|&(id, ..)| id != registration);
}

/// The window procedure a title window had before ours.
const TITLE_PROC: PCWSTR = w!("victory.wndproc");

const WINDOW_FEATURES: PCWSTR = w!("victory.window.features");
const CAPTURE: usize = 1;
const FULLSCREEN: usize = 2;
const ENGLISH: usize = 4;

fn feature_mask(values: [Option<&str>; 3]) -> usize {
    values.iter().enumerate().fold(0, |mask, (i, v)| {
        mask | if *v == Some("0") { 0 } else { 1 << i }
    })
}

fn window_features() -> usize {
    static FLAGS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *FLAGS.get_or_init(|| {
        let values = [
            "VICTORY_MOUSE_CAPTURE",
            "VICTORY_FULLSCREEN",
            "VICTORY_IME_ENGLISH",
        ]
        .map(|key| std::env::var(key).ok());
        feature_mask(values.each_ref().map(|v| v.as_deref()))
    })
}

fn mouse_warp_enabled(capture: bool, value: Option<&str>) -> bool {
    capture && value == Some("1")
}

fn mouse_warp() -> bool {
    static WARP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WARP.get_or_init(|| {
        mouse_warp_enabled(
            window_features() & CAPTURE != 0,
            std::env::var("VICTORY_MOUSE_WARP").ok().as_deref(),
        )
    })
}

fn should_recenter(warp: bool, delta: (i64, i64)) -> bool {
    warp && delta != (0, 0)
}

fn english_layout() {
    static LAYOUT: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let layout = LAYOUT.get_or_init(|| {
        // SAFETY: query the loaded layouts; never install or load a new one.
        unsafe {
            let count = keyboard::GetKeyboardLayoutList(0, std::ptr::null_mut());
            let mut layouts = vec![std::ptr::null_mut(); count.max(0) as usize];
            let read = keyboard::GetKeyboardLayoutList(count, layouts.as_mut_ptr());
            let found = layouts
                .iter()
                .take(read.max(0) as usize)
                .find(|&&h| h as usize & 0xffff_ffff == 0x0409_0409)
                .map(|&h| h as usize);
            if found.is_none() {
                tracing::info!(
                    "en-US keyboard layout unavailable; leaving input language unchanged"
                );
            }
            found
        }
    });
    if let Some(layout) = layout {
        // SAFETY: a loaded layout, applied only to the window's calling thread.
        unsafe { keyboard::ActivateKeyboardLayout(*layout as keyboard::HKL, 0) };
    }
}

#[derive(Default)]
struct MouseCapture {
    window: usize,
    area: RECT,
    hidden: i32,
}
static MOUSE_CAPTURE: Mutex<MouseCapture> = Mutex::new(MouseCapture {
    window: 0,
    area: RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    },
    hidden: 0,
});

fn capture_center(area: &RECT) -> Option<(i32, i32)> {
    (area.right > area.left && area.bottom > area.top).then(|| {
        (
            area.left + (area.right - area.left) / 2,
            area.top + (area.bottom - area.top) / 2,
        )
    })
}

fn capture_motion(point: (i32, i32), area: &RECT) -> (i64, i64) {
    let size = (area.right - area.left, area.bottom - area.top);
    let position = (
        point.0.saturating_sub(area.left),
        point.1.saturating_sub(area.top),
    );
    super::gameinput::mouse_delta(Some((size.0 / 2, size.1 / 2)), Some(position), size).1
}

/// Runs on the window thread; serializes focus/geometry changes with input sampling.
fn capture_mouse(hwnd: HWND, active: bool) {
    if window_features() & CAPTURE == 0 {
        return;
    }
    let mut state = MOUSE_CAPTURE.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: window messages supply hwnd; coordinates and outputs are local.
    unsafe {
        let active = active && wm::GetForegroundWindow() == hwnd && wm::IsIconic(hwnd) == 0;
        if !active {
            if state.window == hwnd as usize {
                wm::ClipCursor(std::ptr::null());
                for _ in 0..state.hidden {
                    wm::ShowCursor(1);
                }
                *state = MouseCapture::default();
            }
            return;
        }
        if state.window != 0 && state.window != hwnd as usize {
            return;
        }
        let (mut area, mut origin) = (RECT::default(), POINT::default());
        if wm::GetClientRect(hwnd, &mut area) == 0 || ClientToScreen(hwnd, &mut origin) == 0 {
            return;
        }
        area.left += origin.x;
        area.right += origin.x;
        area.top += origin.y;
        area.bottom += origin.y;
        let Some(_) = capture_center(&area) else {
            return;
        };
        if wm::ClipCursor(&area) == 0 {
            return;
        }
        if state.window == 0 {
            // Balance exactly our decrements, even if the initial count exceeds zero.
            let extra = (wm::ShowCursor(0) + 1).max(0);
            for _ in 0..extra {
                wm::ShowCursor(0);
            }
            state.hidden = 1 + extra;
            static LOG: std::sync::Once = std::sync::Once::new();
            LOG.call_once(|| {
                tracing::info!(
                    "mouse capture: clipped=<{},{},{},{}> warp={} window={}x{}",
                    area.left,
                    area.top,
                    area.right,
                    area.bottom,
                    mouse_warp(),
                    area.right - area.left,
                    area.bottom - area.top
                );
            });
        }
        state.window = hwnd as usize;
        state.area = area;
    }
}

/// Optional read-before-warp sampling, serialized with focus and geometry changes.
pub(super) fn captured_mouse_delta() -> Option<(i64, i64)> {
    let warp = mouse_warp();
    if !warp {
        return None;
    }
    let state = MOUSE_CAPTURE.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: only the foreground captured window can move the desktop cursor.
    unsafe {
        if state.window == 0 || wm::GetForegroundWindow() as usize != state.window {
            return None;
        }
        let mut point = POINT::default();
        if wm::GetCursorPos(&mut point) == 0 {
            return Some((0, 0));
        }
        let delta = capture_motion((point.x, point.y), &state.area);
        if should_recenter(warp, delta) {
            if let Some(center) = capture_center(&state.area) {
                wm::SetCursorPos(center.0, center.1);
            }
        }
        Some(delta)
    }
}

fn configure_window(hwnd: HWND) {
    let flags = window_features();
    if flags == 0 {
        return;
    } // No new properties, handlers or Win32 effects when disabled.
    // SAFETY: called on the thread that just created this window.
    unsafe {
        let style = wm::GetWindowLongPtrW(hwnd, wm::GWL_STYLE) as u32;
        if style & wm::WS_CHILD != 0 || !wm::GetWindow(hwnd, wm::GW_OWNER).is_null() {
            return;
        }
        if wm::SetPropW(hwnd, WINDOW_FEATURES, flags as *mut c_void) == 0 {
            return;
        }
        if flags & FULLSCREEN != 0 {
            let resolution = std::env::var("VICTORY_FULLSCREEN_RES").ok();
            let size = resolution.as_deref().and_then(parse_window_limit);
            if resolution.is_some() && size.is_none() {
                tracing::info!("invalid VICTORY_FULLSCREEN_RES; using primary monitor size");
            }
            let (width, height) = size.unwrap_or_else(|| {
                (
                    wm::GetSystemMetrics(wm::SM_CXSCREEN),
                    wm::GetSystemMetrics(wm::SM_CYSCREEN),
                )
            });
            wm::SetWindowLongPtrW(
                hwnd,
                wm::GWL_STYLE,
                (style & !(wm::WS_CAPTION | wm::WS_THICKFRAME)) as isize,
            );
            // Window scaling only: preserve the title's fixed 3840x2160 swap chain.
            wm::SetWindowPos(
                hwnd,
                wm::HWND_TOP,
                0,
                0,
                width,
                height,
                wm::SWP_FRAMECHANGED | wm::SWP_NOACTIVATE,
            );
        }
        if flags & ENGLISH != 0 {
            english_layout();
        }
        if flags & CAPTURE != 0 {
            capture_mouse(hwnd, true);
        }
    }
}

pub extern "system" fn CreateWindowExW(
    ex_style: u32,
    class: PCWSTR,
    name: PCWSTR,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    menu: HMENU,
    instance: HINSTANCE,
    param: *const c_void,
) -> HWND {
    let (x, y, width, height) = fit(parent, x, y, width, height);
    // SAFETY: the title's own arguments, forwarded (the size fitted).
    let hwnd = unsafe {
        wm::CreateWindowExW(
            ex_style, class, name, style, x, y, width, height, parent, menu, instance, param,
        )
    };
    hook(hwnd);
    hwnd
}

/// See [`CreateWindowExW`].
pub extern "system" fn CreateWindowExA(
    ex_style: u32,
    class: PCSTR,
    name: PCSTR,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    menu: HMENU,
    instance: HINSTANCE,
    param: *const c_void,
) -> HWND {
    let (x, y, width, height) = fit(parent, x, y, width, height);
    // SAFETY: the title's own arguments, forwarded (the size fitted).
    let hwnd = unsafe {
        wm::CreateWindowExA(
            ex_style, class, name, style, x, y, width, height, parent, menu, instance, param,
        )
    };
    hook(hwnd);
    hwnd
}

/// A console title sizes its window to its render target (
/// the title: 3840x2160 on a Series X), which can exceed the host's screen.
/// A top-level window larger than the work area is shrunk to fit, keeping
/// its proportions, and centred; what it shows is scaled to it (the swap
/// chain stretches). `VICTORY_WINDOW_MAX=WIDTHxHEIGHT` applies a smaller
/// limit without changing the title's render targets.
fn fit(parent: HWND, x: i32, y: i32, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let mut area = RECT::default();
    // SAFETY: `area` receives the primary monitor's work area.
    let known =
        unsafe { wm::SystemParametersInfoW(wm::SPI_GETWORKAREA, 0, (&raw mut area).cast(), 0) };
    let limit = window_limit();
    let fitted = fit_with_limit(&area, width, height, limit);
    if !parent.is_null() || known == 0 || fitted == (width, height) {
        return (x, y, width, height);
    }
    tracing::info!(width, height, to = ?fitted, ?limit, "title window fitted to the screen");
    let (w, h) = fitted;
    let left = area.left + (area.right - area.left - w) / 2;
    let top = area.top + (area.bottom - area.top - h) / 2;
    (left, top, w, h)
}

fn parse_window_limit(value: &str) -> Option<(i32, i32)> {
    let (width, height) = value.trim().split_once(['x', 'X'])?;
    let (width, height) = (width.trim().parse().ok()?, height.trim().parse().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

fn window_limit() -> Option<(i32, i32)> {
    static LIMIT: std::sync::OnceLock<Option<(i32, i32)>> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        let value = std::env::var_os("VICTORY_WINDOW_MAX")?;
        let parsed = value.to_str().and_then(parse_window_limit);
        if parsed.is_none() {
            tracing::warn!(
                ?value,
                "ignoring invalid VICTORY_WINDOW_MAX; expected positive WIDTHxHEIGHT"
            );
        }
        parsed
    })
}

fn fit_with_limit(
    area: &RECT,
    width: i32,
    height: i32,
    limit: Option<(i32, i32)>,
) -> (i32, i32) {
    let Some((max_width, max_height)) = limit else {
        return fit_into(area, width, height);
    };
    let bounded = RECT {
        left: 0,
        top: 0,
        right: (area.right - area.left).min(max_width),
        bottom: (area.bottom - area.top).min(max_height),
    };
    fit_into(&bounded, width, height)
}

/// `width` x `height` scaled down to fit `area`, or unchanged if it fits
/// (or is `CW_USEDEFAULT`).
fn fit_into(area: &RECT, width: i32, height: i32) -> (i32, i32) {
    let (aw, ah) = (area.right - area.left, area.bottom - area.top);
    if width <= 0 || height <= 0 || aw <= 0 || ah <= 0 || (width <= aw && height <= ah) {
        return (width, height);
    }
    let scale = f64::min(
        f64::from(aw) / f64::from(width),
        f64::from(ah) / f64::from(height),
    );
    (
        (f64::from(width) * scale) as i32,
        (f64::from(height) * scale) as i32,
    )
}

/// Puts our procedure in front of the title's.
fn hook(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    // SAFETY: a window this thread just created. The title's procedure is
    // stored before ours can run.
    unsafe {
        let title = wm::GetWindowLongPtrW(hwnd, wm::GWLP_WNDPROC);
        wm::SetPropW(hwnd, TITLE_PROC, title as *mut c_void);
        wm::SetWindowLongPtrW(hwnd, wm::GWLP_WNDPROC, window_proc as *const () as isize);
    }
    configure_window(hwnd);
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: only windows configured above carry these optional handlers.
    let flags = unsafe { wm::GetPropW(hwnd, WINDOW_FEATURES) as usize };
    if flags & CAPTURE != 0 {
        match message {
            wm::WM_KILLFOCUS | wm::WM_NCDESTROY | wm::WM_CANCELMODE | wm::WM_ENTERSIZEMOVE => {
                capture_mouse(hwnd, false)
            }
            wm::WM_ACTIVATE => capture_mouse(hwnd, wparam & 0xffff != wm::WA_INACTIVE as usize),
            wm::WM_SETFOCUS | wm::WM_WINDOWPOSCHANGED | wm::WM_EXITSIZEMOVE => {
                capture_mouse(hwnd, true)
            }
            _ => {}
        }
    }
    if flags & ENGLISH != 0 && message == wm::WM_SETFOCUS {
        english_layout();
    }
    // A user close (close button, Alt+F4, window or taskbar menu); a
    // console title never sees one, nor a WM_CLOSE from it.
    if message == wm::WM_SYSCOMMAND && wparam & 0xFFF0 == wm::SC_CLOSE as usize {
        if flags & CAPTURE != 0 {
            capture_mouse(hwnd, false);
        }
        end_title("window closed", Box::new(|| {}));
        return 0;
    }
    // Alt or F10 puts a window into menu mode: the system runs a modal menu
    // loop on the thread that owns the window, which is the title's. The
    // title then stops (no frames, no input) until a key ends the loop, and
    // nothing shows it, since the window has no menu bar;
    // the title aborts itself about half a minute later. A console has no window
    // menu. The mouse still opens it from the title bar.
    if message == wm::WM_SYSCOMMAND && wparam & 0xFFF0 == wm::SC_KEYMENU as usize {
        return 0;
    }
    // SAFETY: the procedure `hook` stored, called the way the system would.
    unsafe {
        let title = wm::GetPropW(hwnd, TITLE_PROC);
        if message == wm::WM_NCDESTROY {
            wm::RemovePropW(hwnd, TITLE_PROC);
            if flags != 0 {
                wm::RemovePropW(hwnd, WINDOW_FEATURES);
            }
        }
        let title = std::mem::transmute::<*mut c_void, wm::WNDPROC>(title);
        wm::CallWindowProcW(title, hwnd, message, wparam, lparam)
    }
}

/// Suspends the title as the console does when it ends it, runs `then`
/// (once), then ends the process. Returns at once; later calls do nothing.
pub(crate) fn end_title(why: &'static str, then: Box<dyn FnOnce() + Send>) {
    static ENDING: AtomicBool = AtomicBool::new(false);
    if ENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    let constrain = constrained().clone();
    let suspend = routines().clone();
    tracing::info!(
        constrained = constrain.len(),
        routines = suspend.len(),
        "{why}: suspending the title"
    );
    // Whoever comes first (the routines or the deadline) runs `then`; the
    // other waits for it, so the process never ends halfway through.
    let then = std::sync::Arc::new(Mutex::new(Some(then)));
    let finish = move |how: &str| {
        let mut then = then.lock().unwrap_or_else(|e| e.into_inner());
        tracing::info!("{how}; ending the title");
        if let Some(then) = then.take() {
            then();
        }
        end();
    };
    let late = finish.clone();
    // Not on the calling thread: titles hand the suspend to their window
    // thread and wait for it inside the routine.
    std::thread::spawn(move || {
        constrain_then_suspend(&constrain, &suspend);
        finish("title suspended");
    });
    std::thread::spawn(move || {
        std::thread::sleep(SUSPEND_DEADLINE);
        late("title did not finish suspending in time");
    });
}

/// Runs the constrained-change routines (constrained), then the suspend
/// routines (quiesced), as the console does when it suspends a title.
/// Grand Theft Auto VI's suspend routine waits for a state that only its
/// constrained routine brings about; called alone it never returns.
fn constrain_then_suspend(constrain: &[Registration], suspend: &[Registration]) {
    for &(_, routine, context) in constrain.iter().chain(suspend) {
        routine(1, context as *mut c_void);
    }
}

/// Set in the environment of a relaunched `victory`: an inherited handle
/// of the process it replaces.
const RELAUNCHED_FROM: &str = "VICTORY_RELAUNCHED_FROM";

/// Starts `victory` again with `args` (after the title has suspended); the
/// new process waits until this one has ended.
pub(crate) fn relaunch(args: Vec<std::ffi::OsString>) {
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    let mut me: HANDLE = std::ptr::null_mut();
    // SAFETY: an inheritable duplicate of our own process handle.
    let duplicated = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            GetCurrentProcess(),
            GetCurrentProcess(),
            &mut me,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } != 0;
    let spawned = std::env::current_exe().and_then(|exe| {
        let mut command = std::process::Command::new(exe);
        command.args(&args);
        if duplicated {
            command.env(RELAUNCHED_FROM, (me as usize).to_string());
        }
        command.spawn()
    });
    match spawned {
        Ok(child) => tracing::info!(pid = child.id(), ?args, "relaunched"),
        Err(e) => tracing::warn!(%e, "relaunch failed; the title just ends"),
    }
}

/// In a relaunched `victory`: waits (30 s at most) for the process it
/// replaces to be gone, so the two never share the title's files.
pub fn wait_for_previous() {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    let Some(handle) = std::env::var(RELAUNCHED_FROM)
        .ok()
        .and_then(|h| h.parse::<usize>().ok())
    else {
        return;
    };
    // SAFETY: removing our own variable before any other thread reads it.
    unsafe { std::env::remove_var(RELAUNCHED_FROM) };
    let handle = handle as HANDLE;
    // SAFETY: the inherited process handle the previous process gave us.
    unsafe {
        WaitForSingleObject(handle, 30_000);
        CloseHandle(handle);
    }
}

fn end() {
    // Compiled host pipelines kept for the next run.
    #[cfg(windows)]
    victory_gpu::pipeline::save_pipeline_cache();
    // SAFETY: ends this process, as the console ends a suspended title.
    unsafe { TerminateProcess(GetCurrentProcess(), 0) };
}

pub(crate) fn module() -> HleModule {
    HleModule {
        name: "lifecycle",
        dlls: &[
            "api-ms-win-core-psm-appnotify-*",
            "ext-ms-win-rtcore-ntuser-window-*",
        ],
        exports: vec![
            hle_export!(RegisterAppStateChangeNotification),
            hle_export!(UnregisterAppStateChangeNotification),
            hle_export!(RegisterAppConstrainedChangeNotification),
            hle_export!(UnregisterAppConstrainedChangeNotification),
            hle_export!(CreateWindowExW),
            hle_export!(CreateWindowExA),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_features_are_independent_and_disabled_registers_nothing() {
        assert_eq!(feature_mask([None; 3]), 7);
        assert_eq!(feature_mask([Some("0"); 3]), 0);
        for i in 0..3 {
            let mut values = [None; 3];
            values[i] = Some("0");
            assert_eq!(feature_mask(values), 7 & !(1 << i));
        }
    }

    #[test]
    fn capture_center_and_motion_use_screen_coordinates() {
        let area = RECT {
            left: -1920,
            top: 100,
            right: -1,
            bottom: 1181,
        };
        let center = capture_center(&area).unwrap();
        assert_eq!(center, (-961, 640));
        assert_eq!(capture_motion((-950, 620), &area), (11, -20));
        assert_eq!(capture_motion(center, &area), (0, 0));
        assert_eq!(capture_motion((0, 0), &area), (959, -540));
        assert_eq!(capture_motion((i32::MIN, i32::MAX), &area), (-959, 540));
        assert_eq!(capture_motion((1, 1), &RECT::default()), (0, 0));
        assert_eq!(capture_center(&RECT::default()), None);
    }

    #[test]
    fn mouse_warp_requires_capture_opt_in_and_motion() {
        for capture in [false, true] {
            for value in [None, Some("0"), Some("1")] {
                let warp = mouse_warp_enabled(capture, value);
                assert_eq!(warp, capture && value == Some("1"));
                assert!(!should_recenter(warp, (0, 0)));
                for delta in [(1, 0), (0, -1), (-7, 9)] {
                    assert_eq!(should_recenter(warp, delta), warp);
                }
            }
        }
    }

    #[test]
    fn fullscreen_resolution_accepts_only_positive_i32_dimensions() {
        assert_eq!(parse_window_limit("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_window_limit(" 1280 X 720 "), Some((1280, 720)));
        for bad in ["", "0x1080", "-1x720", "1920x", "1x2x3", "2147483648x1"] {
            assert_eq!(parse_window_limit(bad), None);
        }
    }

    extern "system" fn noop(_: u8, _: *mut c_void) {}

    #[test]
    fn registrations_come_and_go() {
        let (mut a, mut b) = (0, 0);
        assert_eq!(
            RegisterAppStateChangeNotification(Some(noop), std::ptr::null_mut(), &mut a),
            0
        );
        assert_eq!(
            RegisterAppStateChangeNotification(Some(noop), std::ptr::null_mut(), &mut b),
            0
        );
        UnregisterAppStateChangeNotification(a);
        let left: Vec<_> = routines().iter().map(|r| r.0).collect();
        assert_eq!(left, [b]);
    }

    #[test]
    fn constrained_registrations_come_and_go() {
        let (mut a, mut b) = (0, 0);
        assert_eq!(
            RegisterAppConstrainedChangeNotification(Some(noop), std::ptr::null_mut(), &mut a),
            0
        );
        assert_eq!(
            RegisterAppConstrainedChangeNotification(Some(noop), std::ptr::null_mut(), &mut b),
            0
        );
        assert_eq!(
            RegisterAppConstrainedChangeNotification(None, std::ptr::null_mut(), &mut a),
            ERROR_INVALID_PARAMETER
        );
        UnregisterAppConstrainedChangeNotification(a);
        let left: Vec<_> = constrained().iter().map(|r| r.0).collect();
        assert_eq!(left, [b]);
    }

    /// What `recording_routine` saw: (context, flag) per call, in order.
    static CALLS: Mutex<Vec<(usize, u8)>> = Mutex::new(Vec::new());

    extern "system" fn recording_routine(flag: u8, context: *mut c_void) {
        CALLS.lock().unwrap().push((context as usize, flag));
    }

    #[test]
    fn constrained_routines_run_before_suspend_routines() {
        let r: Routine = recording_routine;
        constrain_then_suspend(&[(1, r, 10), (2, r, 11)], &[(3, r, 20), (4, r, 21)]);
        assert_eq!(*CALLS.lock().unwrap(), [(10, 1), (11, 1), (20, 1), (21, 1)]);
    }

    #[test]
    fn oversized_windows_shrink_to_the_screen() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        assert_eq!(fit_into(&area, 3840, 2160), (1848, 1040));
        assert_eq!(fit_into(&area, 1280, 720), (1280, 720));
        assert_eq!(
            fit_into(&area, wm::CW_USEDEFAULT, 0),
            (wm::CW_USEDEFAULT, 0)
        );
    }

    #[test]
    fn configured_window_limit_fits_4k_into_1080p() {
        let area = RECT {
            left: 100,
            top: 50,
            right: 2660,
            bottom: 1490,
        };
        assert_eq!(fit_with_limit(&area, 3840, 2160, Some((1920, 1080))), (1920, 1080));
        assert_eq!(fit_with_limit(&area, 640, 480, Some((1920, 1080))), (640, 480));
        assert_eq!(fit_with_limit(&area, 3840, 2160, None), (2560, 1440));
    }

    #[test]
    fn configured_window_limit_still_respects_the_work_area() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        assert_eq!(fit_with_limit(&area, 3840, 2160, Some((2560, 1440))), (1848, 1040));
        assert_eq!(
            fit_with_limit(&area, wm::CW_USEDEFAULT, 0, Some((1920, 1080))),
            (wm::CW_USEDEFAULT, 0)
        );
    }

    #[test]
    fn configured_window_limit_preserves_non_widescreen_proportions() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1440,
        };
        assert_eq!(fit_with_limit(&area, 1600, 1200, Some((1920, 1080))), (1440, 1080));
    }

    #[test]
    fn window_limit_requires_two_positive_dimensions() {
        assert_eq!(parse_window_limit("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_window_limit(" 1920 X 1080 "), Some((1920, 1080)));
        for bad in [
            "",
            "1920",
            "0x1080",
            "1920x0",
            "-1x1080",
            "1920x-1",
            "1920x1080x2",
            "2147483648x1080",
        ] {
            assert_eq!(parse_window_limit(bad), None, "{bad}");
        }
    }

    #[test]
    fn hooked_windows_still_reach_the_title_procedure() {
        let null = std::ptr::null_mut();
        let hwnd = CreateWindowExW(
            0,
            w!("STATIC"),
            w!("before"),
            0,
            0,
            0,
            10,
            10,
            null,
            null,
            null,
            std::ptr::null(),
        );
        assert!(!hwnd.is_null());
        let mut text = [0u16; 16];
        // SAFETY: our window; the buffer holds 16 characters.
        unsafe {
            wm::SetWindowTextW(hwnd, w!("after"));
            let n = wm::GetWindowTextW(hwnd, text.as_mut_ptr(), 16);
            assert_eq!(String::from_utf16_lossy(&text[..n as usize]), "after");
            assert!(!wm::GetPropW(hwnd, TITLE_PROC).is_null());
            assert_eq!(wm::GetPropW(hwnd, WINDOW_FEATURES) as usize, window_features());
            if window_features() & FULLSCREEN != 0 {
                let style = wm::GetWindowLongPtrW(hwnd, wm::GWL_STYLE) as u32;
                assert_eq!(style & (wm::WS_CAPTION | wm::WS_THICKFRAME), 0);
                let mut rect = RECT::default();
                assert_ne!(wm::GetWindowRect(hwnd, &mut rect), 0);
                assert_eq!((rect.left, rect.top), (0, 0));
                if let Ok(res) = std::env::var("VICTORY_FULLSCREEN_RES") {
                    assert_eq!(parse_window_limit(&res), Some((rect.right, rect.bottom)));
                }
            }
            wm::DestroyWindow(hwnd);
        }
    }

    thread_local! {
        /// The system commands `recording` received, low four bits cleared.
        static COMMANDS: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// Stands in for a title's procedure: notes system commands and acts
    /// on none of them (no menu loop, no minimising).
    extern "system" fn recording(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == wm::WM_SYSCOMMAND {
            COMMANDS.with(|c| c.borrow_mut().push(wparam & 0xFFF0));
            return 0;
        }
        // SAFETY: the system's default procedure, called as the system would.
        unsafe { wm::DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    #[test]
    fn the_keyboard_menu_never_reaches_the_title() {
        let null = std::ptr::null_mut();
        let hwnd = CreateWindowExW(
            0,
            w!("STATIC"),
            w!("menu"),
            0,
            0,
            0,
            10,
            10,
            null,
            null,
            null,
            std::ptr::null(),
        );
        assert!(!hwnd.is_null());
        // SAFETY: our window, owned by this thread, so each message is
        // handled before SendMessageW returns. `recording` stands in for the
        // title's procedure and the original is put back before the window
        // is destroyed.
        unsafe {
            let title = wm::GetPropW(hwnd, TITLE_PROC);
            wm::SetPropW(hwnd, TITLE_PROC, recording as *const () as *mut c_void);
            let command = |c: u32, key: u8| {
                wm::SendMessageW(hwnd, wm::WM_SYSCOMMAND, c as usize, LPARAM::from(key))
            };
            command(wm::SC_KEYMENU, 0); // Alt or F10
            command(wm::SC_KEYMENU, b' '); // Alt+Space
            command(wm::SC_KEYMENU | 1, 0); // the low bits are the system's
            command(wm::SC_MINIMIZE, 0);
            wm::SetPropW(hwnd, TITLE_PROC, title);
            wm::DestroyWindow(hwnd);
        }
        let seen = COMMANDS.with(|c| c.borrow().clone());
        assert_eq!(seen, [wm::SC_MINIMIZE as usize]);
    }
}
