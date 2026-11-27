//! The package splash, shown until the title presents steadily.
//!
//! The console shows the `SplashScreenImage` from a title's `ShellVisuals`
//! from launch until the title presents. On the desktop that time showed the
//! title's own window, unpainted. ProjectScarlett creates it for 3840x2160
//! (Wine keeps the width and limits the height to the desktop's), presents a
//! frame about two seconds later and another within the next 26 seconds, and
//! pumps no window messages in between. The resize its first present asks for
//! waits for those messages, so the window stayed wider than the monitor
//! until the title's frames came steadily.
//!
//! An in-context WinEvent hook runs on the thread that creates the title's
//! window, before the title shows it. It fits the window as the first present
//! would and makes it transparent. The splash is a borderless window of
//! TuxBox's own at the image's size, with the image's alpha, so only the image
//! shows. Until the title presents steadily its frames complete without
//! reaching the screen. Then the title's window takes back its own style,
//! before the swap chain exists: removing the layering under a live swap
//! chain leaves Wine's presents slow or occluded for seconds. The swap chain
//! is created after that, and the splash closes with the title's first frame.

// Test builds have no attach initializer, so nothing starts the splash there.
#![cfg_attr(test, allow(dead_code))]
use super::window;
use std::collections::VecDeque;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleHandleExW, GetModuleHandleW,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

/// Series X|S titles present 16:9 frames. The first present fits the window
/// to its back buffer in the same proportion, so it leaves this size alone.
const CONSOLE_OUTPUT: (u32, u32) = (3840, 2160);
/// Frames count as steady once this many arrive within `STEADY_WINDOW`.
const STEADY_FRAMES: usize = 4;
const STEADY_WINDOW: Duration = Duration::from_secs(1);
/// A title that never presents steadily still reaches the screen.
const LONGEST: Duration = Duration::from_secs(60);
/// How long frames wait for the title's thread to take its window's style
/// back. After that the window is made opaque with the style still on.
const UNLAYER_WAIT: Duration = Duration::from_secs(5);
const LARGEST_IMAGE: u32 = 8192;
const CLASS: PCWSTR = w!("TuxBox.Splash");
const CLOSE: u32 = WM_APP;
const UNLAYER: u32 = WM_APP + 1;
/// Closes the splash if the title destroys its window.
const WATCH: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    /// Waiting for the title to create its window.
    Waiting,
    /// The hook is preparing a title window.
    Fitting,
    /// The title window is hidden; the splash thread is opening the splash.
    Claimed,
    /// The splash is up and the title's frames stay behind it.
    Shown,
    /// The title presents steadily; its window is taking its style back.
    Unlayering(Instant),
    /// The title's frames go to the swap chain.
    Released,
    /// The title's window is shown again; the splash closes.
    Ended,
}

struct State {
    stage: Stage,
    /// The title's window, and whether TuxBox made it transparent.
    title: Option<(usize, bool)>,
    /// The splash window while it is open.
    window: Option<usize>,
    /// The title's window has its own style back.
    unlayered: bool,
}

struct Splash {
    state: Mutex<State>,
    changed: Condvar,
    pacing: Mutex<Pacing>,
}

struct Art {
    width: u32,
    height: u32,
    /// Top-down premultiplied BGRA rows.
    pixels: Vec<u8>,
}

static SPLASH: OnceLock<Splash> = OnceLock::new();

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// The splash image a package declares, relative to the package.
pub(super) fn declaration(config: &str) -> Option<String> {
    let start = config.find("<ShellVisuals")? + "<ShellVisuals".len();
    let element = &config[start..];
    let element = &element[..element.find('>')?];
    attributes(element)
        .find(|&(name, _)| name == "SplashScreenImage")
        .map(|(_, image)| image.to_owned())
        .filter(|image| !image.is_empty())
}

/// The `name="value"` pairs of one XML start tag, without entity decoding.
fn attributes(mut rest: &str) -> impl Iterator<Item = (&str, &str)> {
    std::iter::from_fn(move || {
        let (name, after) = rest.split_once('=')?;
        let after = after.trim_start();
        let quote = after.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        let (value, tail) = after[1..].split_once(quote)?;
        rest = tail;
        Some((name.split_whitespace().last().unwrap_or(""), value))
    })
}

/// Where the image goes in an area: centred at its own size, or scaled down
/// in proportion when the area is smaller.
pub(super) fn placement(area: (i32, i32), image: (u32, u32)) -> RECT {
    let (aw, ah) = (i64::from(area.0.max(0)), i64::from(area.1.max(0)));
    let (iw, ih) = (i64::from(image.0.max(1)), i64::from(image.1.max(1)));
    let (w, h) = if iw <= aw && ih <= ah {
        (iw, ih)
    } else if iw * ah > ih * aw {
        (aw, ih * aw / iw)
    } else {
        (iw * ah / ih, ah)
    };
    let (left, top) = ((aw - w) / 2, (ah - h) / 2);
    RECT {
        left: left as i32,
        top: top as i32,
        right: (left + w) as i32,
        bottom: (top + h) as i32,
    }
}

/// Each output index's source indices and weights for an area-weighted
/// reduction from `from` to `to` samples. The weights of one output sum to 1.
fn taps(from: u32, to: u32) -> Vec<Vec<(usize, f32)>> {
    let scale = f64::from(from) / f64::from(to);
    (0..to)
        .map(|i| {
            let (start, end) = (f64::from(i) * scale, f64::from(i + 1) * scale);
            (start.floor() as usize..(end.ceil() as usize).min(from as usize))
                .filter_map(|s| {
                    let cover = end.min(s as f64 + 1.0) - start.max(s as f64);
                    (cover > 0.0).then_some((s, (cover / scale) as f32))
                })
                .collect()
        })
        .collect()
}

/// Scale premultiplied BGRA pixels down, averaging the area each output
/// pixel covers. Premultiplied channels average correctly with their alpha.
pub(super) fn shrink(pixels: &[u8], from: (u32, u32), to: (u32, u32)) -> Option<Vec<u8>> {
    let (fw, fh, tw, th) = (from.0 as usize, from.1 as usize, to.0 as usize, to.1 as usize);
    let average = |sources: &mut dyn Iterator<Item = ([u8; 4], f32)>| {
        let mut sum = [0f32; 4];
        for (pixel, weight) in sources {
            for (total, channel) in sum.iter_mut().zip(pixel) {
                *total += f32::from(channel) * weight;
            }
        }
        sum.map(|total| total.round().clamp(0.0, 255.0) as u8)
    };
    let pixel = |data: &[u8], index: usize| -> [u8; 4] { data[index * 4..][..4].try_into().unwrap() };
    let mut wide = Vec::new();
    wide.try_reserve_exact(fh * tw * 4).ok()?;
    let columns = taps(from.0, to.0);
    for y in 0..fh {
        for taps in &columns {
            wide.extend(average(&mut taps.iter().map(|&(x, w)| (pixel(pixels, y * fw + x), w))));
        }
    }
    let mut result = Vec::new();
    result.try_reserve_exact(tw * th * 4).ok()?;
    for taps in taps(from.1, to.1) {
        for x in 0..tw {
            result.extend(average(&mut taps.iter().map(|&(y, w)| (pixel(&wide, y * tw + x), w))));
        }
    }
    Some(result)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    Hold,
    Steady,
    Timeout,
}

/// When the title's frames stop needing the splash.
#[derive(Default)]
pub(super) struct Pacing {
    first: Option<Instant>,
    recent: VecDeque<Instant>,
    frames: u64,
}

impl Pacing {
    pub(super) fn arrive(&mut self, now: Instant) -> Verdict {
        let first = *self.first.get_or_insert(now);
        self.frames += 1;
        self.recent.push_back(now);
        if self.recent.len() > STEADY_FRAMES {
            self.recent.pop_front();
        }
        if self.recent.len() == STEADY_FRAMES
            && self.recent.front().is_some_and(|&oldest| now - oldest <= STEADY_WINDOW)
        {
            Verdict::Steady
        } else if now - first >= LONGEST {
            Verdict::Timeout
        } else {
            Verdict::Hold
        }
    }
}

/// Start the splash a package declares. Called once as d3d12_xs.dll attaches,
/// on the loading thread before the title's entry point, so the hook exists
/// before the title creates its window.
pub(crate) fn install() {
    let Some(image) = std::env::current_exe().ok().and_then(|exe| {
        let config = std::fs::read_to_string(exe.with_file_name("MicrosoftGame.Config")).ok()?;
        Some(exe.parent()?.join(declaration(&config)?))
    }) else {
        return;
    };
    let splash = SPLASH.get_or_init(|| Splash {
        state: Mutex::new(State { stage: Stage::Waiting, title: None, window: None, unlayered: false }),
        changed: Condvar::new(),
        pacing: Mutex::default(),
    });
    // In-context, the callback runs on the thread that creates each window.
    // The hook names this library, which holds the callback.
    let mut module = HMODULE::default();
    let hook = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR((on_event as *const ()).cast()),
            &mut module,
        )
        .map(|()| {
            SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_CREATE,
                Some(module),
                Some(on_event),
                GetCurrentProcessId(),
                0,
                WINEVENT_INCONTEXT,
            )
        })
        .unwrap_or_default()
    };
    // The thread starts once the loader lock is free, before the title runs.
    let started = !hook.is_invalid()
        && std::thread::Builder::new()
            .name("TuxBox splash".into())
            .spawn(move || run(splash, &image))
            .is_ok();
    if !started {
        lock(&splash.state).stage = Stage::Ended;
    }
}

unsafe extern "system" fn on_event(
    _: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if event != EVENT_OBJECT_CREATE || object != OBJID_WINDOW.0 || child != CHILDID_SELF as i32 {
        return;
    }
    let Some(splash) = SPLASH.get() else { return };
    {
        let mut state = lock(&splash.state);
        if state.stage != Stage::Waiting || !unsafe { is_title_window(hwnd) } {
            return;
        }
        state.stage = Stage::Fitting;
    }
    // This is the window's own thread: the size applies, and the window maps
    // transparent, before the title shows it.
    let _ = window::fit_created_window(hwnd, CONSOLE_OUTPUT.0, CONSOLE_OUTPUT.1);
    let hidden = unsafe { hide(hwnd) };
    let mut state = lock(&splash.state);
    state.title = Some((hwnd.0 as usize, hidden));
    if state.stage == Stage::Fitting {
        state.stage = Stage::Claimed;
    } else if hidden {
        // The title presented meanwhile.
        unsafe { show(hwnd) };
    }
    splash.changed.notify_all();
}

/// A top-level window of the title's own: not a child, owned, message-only,
/// tool or IME window, not one of TuxBox's, and not a small helper.
unsafe fn is_title_window(hwnd: HWND) -> bool {
    unsafe {
        let style = WINDOW_STYLE(GetWindowLongPtrW(hwnd, GWL_STYLE) as u32);
        let extended = WINDOW_EX_STYLE(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32);
        let instance = GetWindowLongPtrW(hwnd, GWLP_HINSTANCE);
        let exe = GetModuleHandleW(None).map_or(0, |m| m.0 as isize);
        let mut name = [0u16; 64];
        let length = GetClassNameW(hwnd, &mut name).max(0) as usize;
        let class = String::from_utf16_lossy(&name[..length]);
        let mut bounds = RECT::default();
        GetAncestor(hwnd, GA_PARENT) == GetDesktopWindow()
            && !style.contains(WS_CHILD)
            && GetWindow(hwnd, GW_OWNER).is_err()
            && !extended.contains(WS_EX_TOOLWINDOW)
            && (instance == 0 || instance == exe)
            && class != "IME"
            && !class.starts_with("TuxBox.")
            && GetWindowRect(hwnd, &mut bounds).is_ok()
            && bounds.right - bounds.left >= 320
            && bounds.bottom - bounds.top >= 200
    }
}

/// Make a window the title created transparent, unless the title layers it
/// itself. A layered window maps only once it has attributes, so a failure
/// takes the style back off.
unsafe fn hide(hwnd: HWND) -> bool {
    unsafe {
        let extended = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if WINDOW_EX_STYLE(extended as u32).contains(WS_EX_LAYERED) {
            return false;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, extended | WS_EX_LAYERED.0 as isize);
        if SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA).is_ok() {
            return true;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, extended);
        false
    }
}

/// Make the title's window opaque again. Wine applies this from any thread
/// at once; taking the style off waits for the title's thread.
unsafe fn show(hwnd: HWND) {
    let _ = unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA) };
}

fn run(splash: &Splash, image: &Path) {
    let art = unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let art = decode(image);
        if com {
            CoUninitialize();
        }
        art
    };
    let opened = match art {
        Ok(art) => open(splash, image, &art),
        Err(error) => {
            crate::diagnostics::log(&format!(
                "package splash {} not shown: {:#010x}",
                image.display(),
                error.code().0
            ));
            false
        }
    };
    if opened {
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
            unsafe { DispatchMessageW(&message) };
        }
    }
    // The splash is closed. The title's window must not stay transparent.
    let (title, revealed) = {
        let mut state = lock(&splash.state);
        let revealed = state.stage == Stage::Ended;
        state.stage = Stage::Ended;
        state.window = None;
        splash.changed.notify_all();
        (state.title.filter(|&(_, hidden)| hidden && !state.unlayered), revealed)
    };
    if let Some((title, _)) = title {
        let title = HWND(title as _);
        unsafe {
            if IsWindow(Some(title)).as_bool() {
                if !revealed {
                    show(title);
                }
                unlayer(splash, title);
            }
        }
    }
}

/// Give the title's window its own style back. This waits until the title's
/// thread takes the message.
unsafe fn unlayer(splash: &Splash, title: HWND) {
    let waited = Instant::now();
    unsafe {
        let extended = GetWindowLongPtrW(title, GWL_EXSTYLE);
        SetWindowLongPtrW(title, GWL_EXSTYLE, extended & !(WS_EX_LAYERED.0 as isize));
    }
    crate::diagnostics::log(&format!(
        "title window took back its own style after {:.1}s",
        waited.elapsed().as_secs_f32()
    ));
    let mut state = lock(&splash.state);
    state.unlayered = true;
    splash.changed.notify_all();
}

/// Wait for the title's window and open the splash over it, unless the
/// splash ends first.
fn open(splash: &Splash, image: &Path, art: &Art) -> bool {
    let title = {
        let mut state = lock(&splash.state);
        loop {
            match (state.stage, state.title) {
                (Stage::Claimed, Some((title, _))) => break HWND(title as _),
                (Stage::Released | Stage::Ended, _) => return false,
                _ => state = splash.changed.wait(state).unwrap_or_else(|e| e.into_inner()),
            }
        }
    };
    let opened = window::work_area(title).and_then(|work| {
        let at = placement((work.right - work.left, work.bottom - work.top), (art.width, art.height));
        let size = ((at.right - at.left) as u32, (at.bottom - at.top) as u32);
        if size.0 == 0 || size.1 == 0 {
            return Err(E_INVALIDARG.into());
        }
        let scaled;
        let pixels = if size == (art.width, art.height) {
            &art.pixels
        } else {
            scaled = shrink(&art.pixels, (art.width, art.height), size).ok_or(E_OUTOFMEMORY)?;
            &scaled
        };
        let origin = POINT { x: work.left + at.left, y: work.top + at.top };
        unsafe { create(origin, size, pixels) }.map(|window| (window, size))
    });
    let (window, size) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            crate::diagnostics::log(&format!("package splash not shown: no window ({:#010x})", error.code().0));
            return false;
        }
    };
    let mut state = lock(&splash.state);
    if state.stage != Stage::Claimed {
        // The title presented while the splash was opening.
        drop(state);
        let _ = unsafe { DestroyWindow(window) };
        return false;
    }
    state.stage = Stage::Shown;
    state.window = Some(window.0 as usize);
    drop(state);
    crate::diagnostics::log(&format!(
        "showing package splash {} ({}x{}) at {}x{}",
        image.file_name().unwrap_or_default().to_string_lossy(),
        art.width,
        art.height,
        size.0,
        size.1
    ));
    true
}

fn decode(path: &Path) -> Result<Art> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: COM is initialized on this thread; the path stays alive for the call.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromFilename(
            PCWSTR(wide.as_ptr()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        let (mut width, mut height) = (0, 0);
        converter.GetSize(&mut width, &mut height)?;
        if width == 0 || height == 0 || width > LARGEST_IMAGE || height > LARGEST_IMAGE {
            return Err(E_INVALIDARG.into());
        }
        let bytes = width as usize * height as usize * 4;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(bytes).map_err(|_| E_OUTOFMEMORY)?;
        pixels.resize(bytes, 0);
        converter.CopyPixels(core::ptr::null(), width * 4, &mut pixels)?;
        Ok(Art { width, height, pixels })
    }
}

/// A borderless window showing premultiplied pixels with their own alpha. It
/// stays off the taskbar and never takes the focus from the title.
unsafe fn create(origin: POINT, size: (u32, u32), pixels: &[u8]) -> Result<HWND> {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(splash_proc),
            hInstance: instance,
            lpszClassName: CLASS,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_win32());
        }
        let (width, height) = (size.0 as i32, size.1 as i32);
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS,
            w!(""),
            WS_POPUP,
            origin.x,
            origin.y,
            width,
            height,
            None,
            None,
            Some(instance),
            None,
        )?;
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = core::ptr::null_mut();
        let shown = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0).and_then(|bitmap| {
            core::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
            let previous = SelectObject(memory, bitmap.into());
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let result = UpdateLayeredWindow(
                window,
                Some(screen),
                Some(&origin as *const _),
                Some(&SIZE { cx: width, cy: height } as *const _),
                Some(memory),
                Some(&POINT::default() as *const _),
                COLORREF(0),
                Some(&blend as *const _),
                ULW_ALPHA,
            );
            SelectObject(memory, previous);
            let _ = DeleteObject(bitmap.into());
            result
        });
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        if let Err(error) = shown {
            let _ = DestroyWindow(window);
            return Err(error);
        }
        let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
        SetTimer(Some(window), WATCH, 250, None);
        Ok(window)
    }
}

unsafe extern "system" fn splash_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: the system calls this on the splash thread, which owns the window.
    unsafe {
        match message {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_TIMER if w.0 == WATCH => {
                let title = SPLASH.get().and_then(|splash| lock(&splash.state).title);
                if title.is_none_or(|(title, _)| !IsWindow(Some(HWND(title as _))).as_bool()) {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            UNLAYER => {
                if let Some(splash) = SPLASH.get() {
                    let title = lock(&splash.state).title;
                    if let Some((title, true)) = title {
                        unlayer(splash, HWND(title as _));
                    }
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, w, l),
        }
    }
}

/// Whether a title frame presented before the swap chain exists stays behind
/// the splash. Once this returns false it keeps doing so.
pub(super) fn hold() -> bool {
    let Some(splash) = SPLASH.get() else {
        return false;
    };
    let mut state = lock(&splash.state);
    let verdict = match state.stage {
        Stage::Released | Stage::Ended => return false,
        Stage::Unlayering(since) => {
            if !state.unlayered {
                if since.elapsed() < UNLAYER_WAIT {
                    return true;
                }
                // The title's thread is not taking messages: show the
                // window with its style still on.
                if let Some((title, true)) = state.title {
                    unsafe { show(HWND(title as _)) };
                }
            }
            state.stage = Stage::Released;
            return false;
        }
        Stage::Shown => lock(&splash.pacing).arrive(Instant::now()),
        // The title presents without a splash up: nothing to hold for.
        _ => Verdict::Steady,
    };
    if verdict == Verdict::Hold {
        return true;
    }
    let shown = state.stage == Stage::Shown;
    match (shown, state.title, state.window) {
        (true, Some((_, true)), Some(window)) => {
            // Frames wait until the splash thread has the style back on the
            // title's window, before any swap chain is created on it.
            state.stage = Stage::Unlayering(Instant::now());
            let _ = unsafe { PostMessageW(Some(HWND(window as _)), UNLAYER, WPARAM(0), LPARAM(0)) };
        }
        _ => {
            if let (false, Some((title, true))) = (shown, state.title) {
                unsafe { show(HWND(title as _)) };
            }
            state.stage = Stage::Released;
        }
    }
    let holding = matches!(state.stage, Stage::Unlayering(_));
    splash.changed.notify_all();
    drop(state);
    let pacing = lock(&splash.pacing);
    crate::diagnostics::log(&match (shown, verdict) {
        (true, Verdict::Steady) => format!(
            "package splash ends at title frame {}, {:.1}s after the first: frames came steadily",
            pacing.frames,
            pacing.first.map_or(0.0, |first| first.elapsed().as_secs_f32())
        ),
        (true, _) => format!(
            "package splash ends at title frame {}: frames did not come steadily within {}s",
            pacing.frames,
            LONGEST.as_secs()
        ),
        _ => "package splash not shown: the title presented before it opened".into(),
    });
    holding
}

/// The title's first frame went to the swap chain: close the splash, waiting
/// briefly until it is gone. The title's window is opaque by now, unless it
/// was hidden when the title presented before the splash opened.
pub(super) fn reveal() {
    let Some(splash) = SPLASH.get() else {
        return;
    };
    let mut state = lock(&splash.state);
    if state.stage == Stage::Ended {
        return;
    }
    state.stage = Stage::Ended;
    if let Some((title, true)) = state.title {
        unsafe { show(HWND(title as _)) };
    }
    if let Some(window) = state.window {
        let _ = unsafe { PostMessageW(Some(HWND(window as _)), CLOSE, WPARAM(0), LPARAM(0)) };
    }
    splash.changed.notify_all();
    let _ = splash
        .changed
        .wait_timeout_while(state, Duration::from_secs(1), |state| state.window.is_some());
}

#[cfg(test)]
#[path = "../tests/present/splash.rs"]
mod tests;
