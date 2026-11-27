//! The F1 settings menu. F1 shows and hides it while our window has the
//! focus; Up/Down pick a setting, Left/Right change it, Enter runs the line
//! picked (save, save and restart, preload shaders, reset, close), Esc
//! closes. Preloading saves, ends this run and has the launcher make every
//! shader before the title starts again (`VICTORY_PRELOAD=1`, `victory
//! preload`); it asks for a second Enter first. While it is
//! shown the title sees no keyboard (`crate::SETTINGS_MENU_OPEN`, read by the
//! input module); a pad still works.
//!
//! The settings are read once at start, so a change is saved to the file
//! `VICTORY_MENU_FILE` names (`scripts\play-foxhound.ps1` sets it to
//! `victory-settings.txt` in the repository's folder and applies it at every
//! start) and takes effect at the next start; "SAVE AND RESTART" starts the
//! launcher again (`VICTORY_MENU_PLAY`, with `VICTORY_MENU_ARGS`) and ends
//! this run. The `VICTORY_MENU_...` settings are the launcher's own choices
//! (DLSS mode, frame generation, frame rate, console, picture size), which it
//! turns into the layer's switches; the others are the switches themselves.
//!
//! In every build, as the stats panel is. Drawn as
//! that panel is: white text on a black box, copied onto the swap chain's
//! buffer, in the middle of the picture.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_DOWN, VK_ESCAPE, VK_F1, VK_LEFT, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
};
/// Q and E: the previous and next tab.
const VK_Q: u16 = 0x51;
const VK_E: u16 = 0x45;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::font::{colours, glyph};

#[path = "menu_look.rs"]
mod look;
use super::{Host, HostList, transition};

const GLYPH: (u32, u32) = (5, 7);
const CELL: (u32, u32) = (6, 9);
const COLUMNS: u32 = 60;
/// Lines of the box: a title, a help line, a blank, the settings shown, a
/// blank, what the line picked does (`ABOUT`, else blank), a note and the
/// repository's link.
const ROWS: usize = 20;
const LINES: u32 = ROWS as u32 + 9;
/// The action that preloads shaders, and what is said of it while it is picked.
const PRELOAD: usize = 2;
const ABOUT_PRELOAD: [&str; 3] = [
    "MAKES EVERY SHADER AHEAD, THEN STARTS THE GAME AGAIN.",
    "TAKES A WHILE: MAYBE A LONG TIME, DEPENDING ON HARDWARE.",
    "A LATER UPDATE OF THIS BUILD MAY NEED A NEW PRELOAD.",
];
/// The setting that turns MFG Unlocker on, and what is said of it while it is picked.
const MFG: &str = "VICTORY_MFG_UNLOCKER";
const ABOUT_MFG: [&str; 3] = [
    "A MOD, NOT OURS: DLSS FRAME GEN ON RTX 20 AND 30 CARDS.",
    "ON: DOWNLOADED FROM ITS GITHUB AT THE NEXT START, THEN RUN.",
    "NVIDIA DOES NOT SUPPORT IT. IF THE GAME FAILS, TURN IT OFF.",
];
/// The note after the first Enter on it: the second one does it.
const CONFIRM_PRELOAD: &str = "ENTER AGAIN: SAVE, CLOSE THE GAME AND PRELOAD";
/// The last line of the box: where this build comes from.
const REPO: &str = "GITHUB.COM/TRUSTNO1-CODER/VICTORY-NVIDIA";
const RIM: u32 = 4;
const UPLOADS: usize = 4;

/// A setting: the variable, its name in the menu, the values it takes
/// (`""`: not set, the build's own default), and whether a change applies at once.
struct Item {
    var: &'static str,
    label: &'static str,
    values: &'static [(&'static str, &'static str)],
    live: bool,
}

const ON_UNSET: &[(&str, &str)] = &[("1", "ON"), ("", "OFF")];
const UNSET_ON: &[(&str, &str)] = &[("", "OFF"), ("1", "ON")];
const STAGES: &[(&str, &str)] = &[("2", "2"), ("1", "1"), ("0", "0 OFF")];

/// The first value of each is what the launcher starts with.
#[rustfmt::skip]
const ITEMS: &[Item] = &[
    // The launcher's own choices.
    Item { var: "VICTORY_MENU_DLSS", label: "DLSS", values: &[("quality", "QUALITY"), ("balanced", "BALANCED"), ("performance", "PERFORMANCE"), ("dlaa", "DLAA (NATIVE)"), ("off", "OFF")], live: false },
    Item { var: "VICTORY_MENU_FRAMEGEN", label: "FRAME GENERATION", values: &[("0", "OFF"), ("2", "2X"), ("3", "3X"), ("4", "4X"), ("5", "5X"), ("6", "6X (MOST)")], live: false },
    // DLSS Frame Generation where it runs (RTX 40 and later), else AMD's FSR 3 (2X only).
    Item { var: "VICTORY_MENU_FG_TYPE", label: "FRAME GEN TYPE", values: &[("auto", "AUTO"), ("dlss", "DLSS"), ("fsr", "FSR 3 (OLDER CARDS)")], live: false },
    // Someone else's mod (apps\victory\src\play.rs): downloaded at the next start, then loaded.
    Item { var: MFG, label: "MFG UNLOCKER (MOD, RTX 20/30)", values: UNSET_ON, live: false },
    Item { var: "VICTORY_MENU_FPS", label: "FRAME RATE CAP", values: &[("1000", "NONE"), ("120", "120"), ("60", "60"), ("30", "30 (THE GAME)")], live: false },
    Item { var: "VICTORY_FPS_LINE", label: "FPS AT THE TOP", values: &[("1", "ON"), ("0", "OFF")], live: true },
    Item { var: "VICTORY_PANEL", label: "STATS PANEL (F3)", values: &[("0", "OFF"), ("1", "ON")], live: true },
    Item { var: "VICTORY_FULLSCREEN", label: "FULLSCREEN (NO BORDER)", values: UNSET_ON, live: false },
    Item { var: "VICTORY_MOUSE_CAPTURE", label: "MOUSE CAPTURE", values: UNSET_ON, live: false },
    Item { var: "VICTORY_IME_ENGLISH", label: "ENGLISH KEYBOARD", values: UNSET_ON, live: false },
    Item { var: "VICTORY_MENU_CONSOLE", label: "CONSOLE", values: &[("series-x", "SERIES X"), ("series-s", "SERIES S (FASTER)")], live: false },
    Item { var: "VICTORY_MENU_RES", label: "SERIES X PICTURE", values: &[("1440", "2560X1440"), ("1080to1440", "1080P TO 1440P"), ("4k", "3840X2160")], live: false },
    // The layer's switches.
    Item { var: "VICTORY_DLSS_PRESET", label: "DLSS MODEL", values: &[("", "DEFAULT"), ("k", "K"), ("j", "J"), ("m", "M"), ("l", "L")], live: false },
    Item { var: "VICTORY_AMBIENT", label: "LIGHT ESTIMATE", values: &[("0.8", "0.8"), ("1", "1"), ("0.5", "0.5"), ("", "OFF")], live: false },
    Item { var: "VICTORY_AMBIENT_SPECULAR", label: "REFLECTION ESTIMATE", values: ON_UNSET, live: false },
    Item { var: "VICTORY_AMBIENT_AIR", label: "FOG CAP", values: &[("1.5", "1.5"), ("1", "1"), ("2", "2"), ("", "NONE")], live: false },
    Item { var: "VICTORY_BVH_BUILDER", label: "RAY TRACING (SLOW, BROKEN)", values: UNSET_ON, live: false },
    Item { var: "VICTORY_WATER_LIGHT", label: "WATER LIGHT", values: &[("", "OFF (NO DARK BLOCKS)"), ("1", "ON (GREEN, DARK BLOCKS)")], live: false },
    Item { var: "VICTORY_LEAVE_OUT", label: "HEAT HAZE AND DECAL DEFRAG", values: &[("02384a60fda6f2d0,731a39171b568298", "LEFT OUT"), ("", "RUN (SLOWER, SMEARS)")], live: false },
    Item { var: "VICTORY_EARLY_DEPTH", label: "EARLY DEPTH TEST", values: ON_UNSET, live: false },
    Item { var: "VICTORY_WC_NO_BACK", label: "SKIP BIG COPY-BACKS", values: ON_UNSET, live: false },
    Item { var: "VICTORY_SOFT_VIEWS", label: "SOFT VIEWS (NO CUT FREEZES)", values: &[("copies", "ON"), ("", "OFF")], live: false },
    Item { var: "VICTORY_TEXTURE_NO_WAIT", label: "TEXTURE NO WAIT", values: ON_UNSET, live: false },
    Item { var: "VICTORY_RELAY", label: "RELAY (SPEED, TRIANGLES)", values: STAGES, live: false },
    Item { var: "VICTORY_CARD_STEPS", label: "CARD STEPS", values: STAGES, live: false },
    Item { var: "VICTORY_INDIRECT_BATCH", label: "INDIRECT BATCH", values: STAGES, live: false },
    Item { var: "VICTORY_CARD_BUFFERS_MIN_KB", label: "CARD COMPARES FROM (KB)", values: &[("64", "64"), ("1024", "1024")], live: false },
    Item { var: "VICTORY_PORT_CONSTANTS", label: "CONSTANTS HEAP IN PLACE", values: UNSET_ON, live: false },
];
/// The lines after the settings.
const ACTIONS: [&str; 5] =
    ["SAVE AND RESTART NOW", "SAVE FOR NEXT START", "PRELOAD SHADERS (RESTARTS THE GAME)", "RESET ALL TO PLAY DEFAULTS", "CLOSE"];

/// The menu's tabs, and the tab of each setting (the rest are under ADVANCED).
const TABS: [&str; 5] = ["DISPLAY", "GRAPHICS", "PERFORMANCE", "CONTROLS", "ADVANCED"];
fn tab_of(var: &str) -> usize {
    match var {
        "VICTORY_FULLSCREEN" | "VICTORY_MENU_CONSOLE" | "VICTORY_MENU_RES" | "VICTORY_FPS_LINE" | "VICTORY_PANEL" => 0,
        "VICTORY_MENU_DLSS" | "VICTORY_DLSS_PRESET" | "VICTORY_AMBIENT" | "VICTORY_AMBIENT_SPECULAR" | "VICTORY_AMBIENT_AIR" | "VICTORY_BVH_BUILDER"
        | "VICTORY_WATER_LIGHT" | "VICTORY_LEAVE_OUT" => 1,
        "VICTORY_MENU_FRAMEGEN" | "VICTORY_MENU_FG_TYPE" | MFG | "VICTORY_MENU_FPS" | "VICTORY_EARLY_DEPTH" | "VICTORY_WC_NO_BACK" | "VICTORY_SOFT_VIEWS"
        | "VICTORY_TEXTURE_NO_WAIT" => 2,
        "VICTORY_MOUSE_CAPTURE" | "VICTORY_IME_ENGLISH" => 3,
        _ => 4,
    }
}

/// The settings of `tab`, by their index in `ITEMS`.
fn tab_items(tab: usize) -> Vec<usize> {
    (0..ITEMS.len()).filter(|&i| tab_of(ITEMS[i].var) == tab).collect()
}

/// Upper-case menu words as a title has them ("FRAME RATE CAP" -> "Frame Rate Cap"),
/// with names and sizes left as they are (DLSS, RTX; 3440X1440 -> 3440x1440).
fn pretty(text: &str) -> String {
    const KEEP: [&str; 15] = ["DLSS", "DLAA", "FPS", "FG", "MFG", "RTX", "FSR", "AMD", "KB", "F1", "F3", "X", "S", "UI", "GPU"];
    text.split(' ')
        .map(|word| {
            let core: String = word.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            if KEEP.contains(&core.as_str()) {
                word.to_owned()
            } else if word.chars().any(|c| c.is_ascii_digit()) {
                word.replace('X', "x")
            } else {
                let mut first = true;
                word.chars()
                    .map(|c| {
                        if c.is_ascii_alphabetic() {
                            let out = if first { c } else { c.to_ascii_lowercase() };
                            first = false;
                            out
                        } else {
                            c
                        }
                    })
                    .collect()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The actions' short names on their buttons.
fn action_label(action: &str) -> &str {
    match action {
        "SAVE AND RESTART NOW" => "SAVE AND RESTART",
        "SAVE FOR NEXT START" => "SAVE",
        "PRELOAD SHADERS (RESTARTS THE GAME)" => "PRELOAD SHADERS",
        "RESET ALL TO PLAY DEFAULTS" => "RESET",
        other => other,
    }
}

/// An upper-case sentence as a sentence ("TAKES A WHILE." -> "Takes a while."), names kept.
fn pretty_sentence(text: &str) -> String {
    let words = pretty(text);
    let mut out = String::new();
    let mut start = true;
    for word in words.split(' ') {
        let keep = word.chars().filter(|c| c.is_ascii_uppercase()).count() > 1 || word.chars().any(|c| c.is_ascii_digit());
        let w = if start || keep { word.to_owned() } else { word.to_ascii_lowercase() };
        start = word.ends_with('.') || word.ends_with(':');
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&w);
    }
    out
}

fn get(var: &str) -> String {
    std::env::var(var).unwrap_or_default()
}

/// The first start of a new victory.exe: the seconds it plays before it starts
/// again by itself (`VICTORY_WARMUP_RESTART`, set by the launcher while the exe
/// has no mark under `%LOCALAPPDATA%\victory\warmed`). On the RTX 5070, every
/// new build's first run stayed at a flat 14 fps (Present ~40 ms) for as long as
/// it ran, and every later run was normal (27-29 fps), even after a first run of
/// 45 seconds; a copy of an exe already run was fast at once (2026-11-27). Likely
/// the driver's cache of the program; not ours to fix, so it is run through once.
pub(crate) fn warmup_seconds() -> Option<u64> {
    get("VICTORY_WARMUP_RESTART").parse().ok().filter(|s| *s > 0)
}

/// The start's notice lines for the warm-up start ([`warmup_seconds`]).
pub(crate) fn warmup_notice(seconds: u64) -> Vec<String> {
    vec![
        "FIRST START OF THIS VERSION: A ONE-TIME WARM-UP".to_owned(),
        format!("THE GAME RESTARTS BY ITSELF IN ABOUT {seconds} SECONDS"),
        "THIS HAPPENS ONCE PER NEW VERSION, AND IS NEEDED:".to_owned(),
        "WITHOUT IT THE FIRST RUN STAYS AT ABOUT 14 FPS".to_owned(),
        String::new(),
    ]
}

/// Starts `command_line` in a console window of its own, with that console's input and output.
/// (`std::process::Command` always hands its child this process's standard handles: the restarted
/// launcher then wrote into the old launcher's pipe and log, not its own window, and kept the old
/// launcher waiting on that pipe.)
fn new_console(command_line: &str) -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_CONSOLE, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};
    let mut line: Vec<u16> = command_line.encode_utf16().chain(Some(0)).collect();
    // SAFETY: plain structs, zero is their empty value.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `line` is a writable, terminated UTF-16 string; no handles inherited; this
    // process's environment and folder.
    let made = unsafe {
        CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NEW_CONSOLE,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    if made == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the two handles CreateProcessW gave, closed once.
    unsafe {
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    Ok(())
}

/// Whether a change of `item` shows at once. Frame generation does when this run started with
/// it ready (the launcher asks for that whenever Streamline is there).
fn live(item: &Item) -> bool {
    item.live || (item.var == "VICTORY_MENU_FRAMEGEN" && crate::streamline::framegen_live())
}

/// Whether the frame-rate line is shown at the top while the menu is closed
/// (`VICTORY_FPS_LINE=0`: not; the menu changes it).
fn fps_line_on() -> &'static AtomicBool {
    static ON: OnceLock<AtomicBool> = OnceLock::new();
    ON.get_or_init(|| AtomicBool::new(std::env::var_os("VICTORY_FPS_LINE").is_none_or(|v| v != "0")))
}

/// That line: the frames rendered a second and, while frame generation shows more, those shown.
fn fps_line() -> Option<String> {
    if !fps_line_on().load(Ordering::Relaxed) {
        return None;
    }
    // The stats panel has that corner (F3), and says the same.
    if super::panel::shown().load(Ordering::Relaxed) {
        return None;
    }
    let fps = f64::from(super::METER_FPS_TENTHS.load(Ordering::Relaxed)) / 10.0;
    if fps <= 0.0 {
        return None;
    }
    Some(match crate::streamline::shown_per_frame() {
        1 => format!("FPS: {fps:.1}"),
        per => format!("FPS: {fps:.1}  FG: {:.1}", fps * f64::from(per)),
    })
}

/// The index of `value` among the item's values (an unknown value: shown as
/// itself, at the end).
fn index_of(item: &Item, value: &str) -> Option<usize> {
    item.values.iter().position(|(v, _)| *v == value)
}

/// What the menu shows for `value`: its label, an unknown value as itself,
/// and a value the launcher did not set (a start without play.ps1) as
/// "NOT SET" rather than a blank.
fn shown_value(item: &Item, value: &str) -> String {
    match index_of(item, value) {
        Some(i) => item.values[i].1.to_owned(),
        None if value.is_empty() => "NOT SET".to_owned(),
        None => value.to_ascii_uppercase(),
    }
}

/// Keys pressed, gathered on a thread of their own every 10 ms: a frame
/// lasts 50 ms and more, and a short press would fall between two frames.
fn keys() -> &'static Mutex<Vec<u16>> {
    static KEYS: OnceLock<Mutex<Vec<u16>>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let _ = std::thread::Builder::new().name("settings menu keys".into()).spawn(|| {
            const WATCHED: [u16; 10] = [VK_F1, VK_UP, VK_DOWN, VK_LEFT, VK_RIGHT, VK_RETURN, VK_ESCAPE, VK_Q, VK_E, VK_TAB];
            let mut was = [false; WATCHED.len()];
            loop {
                std::thread::sleep(Duration::from_millis(10));
                let mut process = 0;
                // SAFETY: plain queries; room for the answer.
                unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut process) };
                let ours = process == std::process::id();
                for (i, &vk) in WATCHED.iter().enumerate() {
                    // SAFETY: a plain query.
                    let down = ours && unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
                    if down && !was[i] {
                        keys().lock().unwrap_or_else(|e| e.into_inner()).push(vk);
                    }
                    was[i] = down;
                }
            }
        });
        Mutex::new(Vec::new())
    })
}

/// The box's size in pixels.
const fn size(scale: u32) -> (u32, u32) {
    ((COLUMNS * CELL.0 + 2 * RIM) * scale, (LINES * CELL.1 + 2 * RIM) * scale)
}

fn lit(lines: &[String], x: u32, y: u32) -> bool {
    let (Some(x), Some(y)) = (x.checked_sub(RIM), y.checked_sub(RIM)) else {
        return false;
    };
    if x % CELL.0 >= GLYPH.0 || y % CELL.1 >= GLYPH.1 || x / CELL.0 >= COLUMNS {
        return false;
    }
    let Some(&c) = lines.get((y / CELL.1) as usize).and_then(|l| l.as_bytes().get((x / CELL.0) as usize)) else {
        return false;
    };
    (glyph(c.to_ascii_uppercase())[(y % CELL.1) as usize] >> (GLYPH.0 - 1 - x % CELL.0)) & 1 != 0
}

/// `rgb` (sRGB, 0 to 1) in the swap chain's `format`, into `out` (its bytes).
fn encode(format: DXGI_FORMAT, rgb: [f32; 3], out: &mut [u8]) {
    let unorm = |c: f32, max: f32| (c.clamp(0.0, 1.0) * max).round() as u32;
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM => out.copy_from_slice(&[unorm(rgb[0], 255.0) as u8, unorm(rgb[1], 255.0) as u8, unorm(rgb[2], 255.0) as u8, 0xFF]),
        DXGI_FORMAT_B8G8R8A8_UNORM => out.copy_from_slice(&[unorm(rgb[2], 255.0) as u8, unorm(rgb[1], 255.0) as u8, unorm(rgb[0], 255.0) as u8, 0xFF]),
        DXGI_FORMAT_R10G10B10A2_UNORM => {
            let word = unorm(rgb[0], 1023.0) | unorm(rgb[1], 1023.0) << 10 | unorm(rgb[2], 1023.0) << 20 | 3 << 30;
            out.copy_from_slice(&word.to_le_bytes());
        }
        // Linear light (scRGB), as halves.
        DXGI_FORMAT_R16G16B16A16_FLOAT => {
            let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            for (i, c) in [linear(rgb[0]), linear(rgb[1]), linear(rgb[2]), 1.0].into_iter().enumerate() {
                out[2 * i..2 * i + 2].copy_from_slice(&half(c).to_le_bytes());
            }
        }
        _ => {}
    }
}

/// `value` (0 to 1) as a half float.
fn half(value: f32) -> u16 {
    let bits = value.to_bits();
    let exponent = ((bits >> 23) & 0xFF) as i32 - 127 + 15;
    if value <= 0.0 || exponent <= 0 {
        return 0;
    }
    ((exponent as u32) << 10 | (bits >> 13) & 0x3FF) as u16
}

/// The frame-rate line: white text on black, in the corner.
fn painted(lines: &[String], scale: u32, colours: [&[u8]; 2], pitch: usize, rows: usize) -> Vec<u8> {
    let (width, height) = size(scale);
    let bytes = colours[0].len();
    let mut out = vec![0; pitch * rows];
    for y in 0..height / scale {
        let top = pitch * (y * scale) as usize;
        for x in 0..width / scale {
            let colour = colours[usize::from(lit(lines, x, y))];
            for i in 0..scale {
                let at = top + (x * scale + i) as usize * bytes;
                out[at..at + bytes].copy_from_slice(colour);
            }
        }
        for i in 1..scale as usize {
            out.copy_within(top..top + pitch, top + i * pitch);
        }
    }
    out
}

/// The menu's picture (0x00RRGGBB, `width` a row) in the swap chain's format, rows `pitch` apart.
fn converted(pixels: &[u32], width: usize, format: DXGI_FORMAT, bytes: usize, pitch: usize, rows: usize) -> Vec<u8> {
    let mut out = vec![0; pitch * rows];
    let mut cache: Option<(u32, [u8; 8])> = None;
    for (y, row) in pixels.chunks(width).enumerate() {
        for (x, &p) in row.iter().enumerate() {
            let at = y * pitch + x * bytes;
            let encoded = match cache {
                Some((q, e)) if q == p => e,
                _ => {
                    let rgb = [(p >> 16 & 0xFF) as f32 / 255.0, (p >> 8 & 0xFF) as f32 / 255.0, (p & 0xFF) as f32 / 255.0];
                    let mut e = [0u8; 8];
                    encode(format, rgb, &mut e[..bytes]);
                    cache = Some((p, e));
                    e
                }
            };
            out[at..at + bytes].copy_from_slice(&encoded[..bytes]);
        }
    }
    out
}

struct Buffers {
    format: DXGI_FORMAT,
    /// The size held: the menu's picture or the frame-rate line, whichever is larger.
    size: (u32, u32),
    pitch: u32,
    uploads: Vec<(ID3D12Resource, *mut u8)>,
    card: ID3D12Resource,
    turn: usize,
    fresh: bool,
    /// The card's buffer was copied into before (it is in the copy-source state;
    /// at first it is in the common state, promoted by the first copy).
    used: bool,
}

// SAFETY: free-threaded resources, and memory only written under the presenter's lock.
unsafe impl Send for Buffers {}

impl Buffers {
    fn new(host: &Host, format: DXGI_FORMAT, (width, height): (u32, u32)) -> Option<Buffers> {
        let pitch = (width * colours(format)?[0].len() as u32).next_multiple_of(256);
        let bytes = u64::from(pitch) * u64::from(height);
        let uploads = (0..UPLOADS).map(|_| host.upload_buffer(bytes)).collect::<Option<Vec<_>>>()?;
        let heap = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut card: Option<ID3D12Resource> = None;
        // SAFETY: plain creation of a buffer.
        unsafe {
            host.device
                .CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COMMON, None, &mut card)
                .ok()?;
        }
        Some(Buffers { format, size: (width, height), pitch, uploads, card: card?, turn: 0, fresh: false, used: false })
    }

    fn write(&mut self, picture: Vec<u8>) {
        self.turn = (self.turn + 1) % UPLOADS;
        // SAFETY: a mapped upload buffer of the picture's size, which the card read frames ago.
        unsafe { std::ptr::copy_nonoverlapping(picture.as_ptr(), self.uploads[self.turn].1, picture.len()) };
        self.fresh = true;
    }
}

/// The menu of a presenter.
pub(super) struct Menu {
    /// The values chosen, by item (as at start until changed).
    values: Vec<String>,
    /// The values at start.
    started: Vec<String>,
    /// The line picked: an item, or an action after them.
    at: usize,
    note: String,
    changed: bool,
    /// What the buffer holds: the frame-rate line, or (none) the menu's lines.
    painted: Option<String>,
    buffers: Option<Buffers>,
    failed: bool,
    /// The tab shown.
    tab: usize,
}

impl Menu {
    pub(super) fn new() -> Menu {
        // Start the key thread now: F1 must be seen while the menu is closed.
        let _ = keys();
        // `VICTORY_MENU_OPEN=1`: shown from the start (to look at it in a run nobody presses keys in).
        if std::env::var_os("VICTORY_MENU_OPEN").is_some_and(|v| v == "1") {
            crate::SETTINGS_MENU_OPEN.store(true, Ordering::Relaxed);
        }
        let values: Vec<String> = ITEMS.iter().map(|i| get(i.var)).collect();
        Menu { started: values.clone(), values, at: tab_items(0)[0], note: String::new(), changed: true, painted: None, buffers: None, failed: false, tab: 0 }
    }

    fn open() -> bool {
        crate::SETTINGS_MENU_OPEN.load(Ordering::Relaxed)
    }

    /// The menu as lines of text (its words, checked by the tests; the picture is `picture`).
    #[cfg(test)]
    fn lines(&self) -> Vec<String> {
        let total = ITEMS.len() + ACTIONS.len();
        let first = self.at.saturating_sub(ROWS / 2).min(total.saturating_sub(ROWS));
        let mut out = vec![
            "SETTINGS                       F1 OR ESC CLOSES".to_owned(),
            "UP/DOWN PICK  LEFT/RIGHT CHANGE  ENTER DOES IT".to_owned(),
            String::new(),
        ];
        for row in first..(first + ROWS).min(total) {
            let cursor = if row == self.at { '>' } else { ' ' };
            let line = if let Some(item) = ITEMS.get(row) {
                let value = &self.values[row];
                let shown = shown_value(item, value);
                let mark = if *value != self.started[row] { if live(item) { " *LIVE" } else { " *" } } else if live(item) { " LIVE" } else { "" };
                format!("{cursor} {:<27} < {shown} >{mark}", item.label)
            } else {
                format!("{cursor} {}", ACTIONS[row - ITEMS.len()])
            };
            out.push(line.chars().take(COLUMNS as usize).collect());
        }
        while out.len() < 3 + ROWS {
            out.push(String::new());
        }
        out.push(String::new());
        let about = if self.at == ITEMS.len() + PRELOAD {
            ABOUT_PRELOAD
        } else if ITEMS.get(self.at).is_some_and(|i| i.var == MFG) {
            ABOUT_MFG
        } else {
            [""; 3]
        };
        out.extend(about.iter().map(|l| (*l).to_owned()));
        out.push(if self.note.is_empty() { "* = CHANGED: APPLIES AT THE NEXT START UNLESS LIVE".to_owned() } else { self.note.clone() });
        out.push(REPO.to_owned());
        out
    }

    /// Changes item `i` by `step` (+1 / -1) through its values.
    fn change(&mut self, i: usize, step: isize) {
        let item = &ITEMS[i];
        let n = item.values.len() as isize;
        let now = index_of(item, &self.values[i]).map_or(-1, |x| x as isize);
        let next = if now < 0 { if step > 0 { 0 } else { n - 1 } } else { (now + step).rem_euclid(n) };
        self.values[i] = item.values[next as usize].0.to_owned();
        if live(item) {
            self.apply_live(i);
        }
        self.note.clear();
    }

    /// A setting the build reads at each use: changed in this run's environment at once.
    fn apply_live(&self, i: usize) {
        let (var, value) = (ITEMS[i].var, &self.values[i]);
        // SAFETY: the variables changed here are read by this build with
        // std::env::var at each use; Windows' environment block is locked by
        // the process heap's routines, and std holds its own lock.
        unsafe {
            if value.is_empty() {
                std::env::remove_var(var);
            } else {
                std::env::set_var(var, value);
            }
        }
        match var {
            // Off (0) or the frames shown for each one rendered, from the next present on.
            "VICTORY_MENU_FRAMEGEN" => crate::streamline::set_framegen(value.parse().unwrap_or(0)),
            "VICTORY_FPS_LINE" => fps_line_on().store(value != "0", Ordering::Relaxed),
            "VICTORY_PANEL" => super::panel::shown().store(value == "1", Ordering::Relaxed),
            _ => {}
        }
    }

    /// Writes the changed values over those in the settings file.
    fn save(&mut self) -> bool {
        let Some(path) = std::env::var_os("VICTORY_MENU_FILE") else {
            self.note = "NO SETTINGS FILE: START WITH VICTORY PLAY".to_owned();
            return false;
        };
        let mut kept: Vec<(String, String)> = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())))
            .filter(|(k, _)| k.starts_with("VICTORY_"))
            .collect();
        for (i, item) in ITEMS.iter().enumerate() {
            if self.values[i] == self.started[i] {
                continue;
            }
            kept.retain(|(k, _)| k != item.var);
            kept.push((item.var.to_owned(), self.values[i].clone()));
        }
        let mut text = String::from("# Saved by the F1 menu; applied over the play settings at every start (an empty value: unset).\r\n");
        for (k, v) in &kept {
            text.push_str(&format!("{k}={v}\r\n"));
        }
        match std::fs::write(&path, text) {
            Ok(()) => {
                tracing::info!(path = %std::path::Path::new(&path).display(), "settings menu: saved");
                self.started = self.values.clone();
                self.note = "SAVED. APPLIES AT THE NEXT START".to_owned();
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "settings menu: not saved");
                self.note = "COULD NOT SAVE THE SETTINGS FILE".to_owned();
                false
            }
        }
    }

    /// Starts the launcher again in a new console, a few seconds on, and ends this run.
    /// `preload`: the launcher makes every shader first (`VICTORY_PRELOAD=1`, for that start alone).
    fn restart(&mut self, preload: bool) {
        let Some(play) = std::env::var_os("VICTORY_MENU_PLAY") else {
            self.note = "SAVED. START THE LAUNCHER AGAIN YOURSELF".to_owned();
            return;
        };
        let play = play.to_string_lossy().replace('\'', "''");
        let args = get("VICTORY_MENU_ARGS");
        // The preload is asked by VICTORY_PRELOAD=1, which both launchers read (victory.exe's is
        // `--preload`, the script's `-Preload`: the script's flag given to victory.exe was refused,
        // and the window closed before it could be read, 2026-11-27). A launcher that fails keeps
        // its window until Enter.
        let command = format!(
            "Start-Sleep 3; {}& '{play}' {args}; if ($LASTEXITCODE) {{ Read-Host 'The start failed (see above). Press Enter to close this window' }}",
            if preload { "$env:VICTORY_PRELOAD='1'; " } else { "" }
        );
        match new_console(&format!("powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \"{}\"", command.replace('"', "\\\""))) {
            Ok(()) => {
                tracing::info!(command, preload, "settings menu: restarting");
                // Ended at once: std::process::exit runs the DLLs' unloading, which with
                // Streamline loaded did not come back (a windowless leftover process).
                // SAFETY: this process, with nothing of its own left to save.
                unsafe {
                    use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
                    TerminateProcess(GetCurrentProcess(), 0);
                }
                std::process::exit(0);
            }
            Err(e) => {
                tracing::warn!(error = %e, "settings menu: restart failed");
                self.note = "SAVED, BUT THE RESTART FAILED".to_owned();
            }
        }
    }

    /// The warm-up start ([`warmup_seconds`]): once its time is up, this exe is
    /// marked as warmed up (`VICTORY_WARMUP_MARK`) and the game starts again.
    fn warmup(&mut self) {
        static BEGAN: OnceLock<std::time::Instant> = OnceLock::new();
        static DONE: AtomicBool = AtomicBool::new(false);
        let Some(seconds) = warmup_seconds() else { return };
        if DONE.load(Ordering::Relaxed) {
            return;
        }
        if BEGAN.get_or_init(std::time::Instant::now).elapsed() < Duration::from_secs(seconds) {
            return;
        }
        DONE.store(true, Ordering::Relaxed);
        let mark = std::path::PathBuf::from(get("VICTORY_WARMUP_MARK"));
        if !mark.as_os_str().is_empty() {
            if let Some(dir) = mark.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&mark, "this victory.exe was warmed up (its first start, then a restart)\r\n");
        }
        tracing::info!(seconds, mark = %mark.display(), "first start of this victory.exe: warmed up; the game starts again");
        self.restart(false);
    }

    /// Removes the settings file: the launcher's own settings at the next start.
    fn to_tab(&mut self, tab: usize) {
        self.tab = tab % TABS.len();
        self.at = tab_items(self.tab).first().copied().unwrap_or(ITEMS.len());
    }

    /// Up (-1) or down (+1) through the tab's settings, then its actions (one row).
    fn step(&mut self, by: isize) {
        let mut rows = tab_items(self.tab);
        rows.push(ITEMS.len());
        let here = if self.at >= ITEMS.len() { rows.len() - 1 } else { rows.iter().position(|&r| r == self.at).unwrap_or(0) };
        let next = (here as isize + by).rem_euclid(rows.len() as isize) as usize;
        // Onto the actions: the first one (or the one already picked).
        self.at = if rows[next] == ITEMS.len() && self.at >= ITEMS.len() { self.at } else { rows[next] };
    }

    /// What the picture shows now: the rows, the first shown, the action picked, and the description.
    fn page_parts(&self) -> (Vec<look::Row>, usize, Option<usize>, String, Vec<String>) {
        let items = tab_items(self.tab);
        let rows: Vec<look::Row> = items
            .iter()
            .map(|&i| {
                let item = &ITEMS[i];
                let value = &self.values[i];
                let toggle = item.values.len() == 2 && item.values.iter().all(|(_, l)| *l == "ON" || *l == "OFF");
                let value = if toggle {
                    look::Value::Toggle(shown_value(item, value) == "ON")
                } else {
                    let at = index_of(item, value).map_or(item.values.len(), |x| x + 1);
                    look::Value::Pick(pretty(&shown_value(item, value)), at, item.values.len())
                };
                look::Row { label: pretty(item.label), value, picked: i == self.at, changed: self.values[i] != self.started[i] }
            })
            .collect();
        let picked = items.iter().position(|&i| i == self.at).unwrap_or(0);
        let first = picked.saturating_sub(look::ROWS - 1).min(rows.len().saturating_sub(look::ROWS));
        let action = self.at.checked_sub(ITEMS.len());
        let sentence = |lines: &[&str]| vec![pretty_sentence(&lines.join(" "))];
        let (heading, about) = match (ITEMS.get(self.at), action) {
            (Some(item), _) => {
                let mut about = if item.var == MFG { sentence(&ABOUT_MFG) } else { Vec::new() };
                about.push(if live(item) { "Changes right away.".to_owned() } else { "Applies at the next start: pick Save And Restart.".to_owned() });
                about.push(format!("Now: {}", pretty(&shown_value(item, &self.values[self.at]))));
                (pretty(item.label), about)
            }
            (None, Some(PRELOAD)) => (pretty(action_label(ACTIONS[PRELOAD])), sentence(&ABOUT_PRELOAD)),
            (None, Some(a)) => (pretty(action_label(ACTIONS[a])), Vec::new()),
            (None, None) => (String::new(), Vec::new()),
        };
        (rows, first, action, heading, about)
    }

    fn picture(&self, s: f32) -> Option<Vec<u32>> {
        let (rows, first, action, heading, about) = self.page_parts();
        let actions: Vec<String> = ACTIONS.iter().map(|a| pretty(action_label(a))).collect();
        let actions: Vec<&str> = actions.iter().map(String::as_str).collect();
        let note = if self.note.is_empty() {
            "* Changed: applies at the next start, unless it changes right away".to_owned()
        } else {
            pretty_sentence(&self.note)
        };
        let page = look::Page { tabs: &TABS, tab: self.tab, rows: &rows, first, actions: &actions, action, heading, about, note, link: &REPO.to_ascii_lowercase() };
        look::draw(&page, s)
    }

    fn reset(&mut self) {
        if let Some(path) = std::env::var_os("VICTORY_MENU_FILE") {
            let _ = std::fs::remove_file(path);
        }
        self.note = "RESET. PLAY DEFAULTS AT THE NEXT START".to_owned();
    }

    /// A frame was presented: the keys pressed since the last one.
    pub(super) fn frame(&mut self) {
        self.warmup();
        let pressed = std::mem::take(&mut *keys().lock().unwrap_or_else(|e| e.into_inner()));
        if pressed.is_empty() {
            return;
        }
        for vk in pressed {
            let open = Self::open();
            if vk == VK_F1 || (open && vk == VK_ESCAPE) {
                crate::SETTINGS_MENU_OPEN.store(!open && vk == VK_F1, Ordering::Relaxed);
                self.note.clear();
                self.changed = true;
                continue;
            }
            if !open {
                continue;
            }
            // Only an Enter right after the first one starts a preload.
            let confirmed = self.note == CONFIRM_PRELOAD;
            if confirmed {
                self.note.clear();
            }
            match vk {
                VK_Q => self.to_tab(self.tab + TABS.len() - 1),
                VK_E | VK_TAB => self.to_tab(self.tab + 1),
                VK_UP => self.step(-1),
                VK_DOWN => self.step(1),
                VK_LEFT if self.at < ITEMS.len() => self.change(self.at, -1),
                VK_RIGHT if self.at < ITEMS.len() => self.change(self.at, 1),
                // On the actions, left and right go from one to the next.
                VK_LEFT => self.at = ITEMS.len() + (self.at - ITEMS.len() + ACTIONS.len() - 1) % ACTIONS.len(),
                VK_RIGHT => self.at = ITEMS.len() + (self.at - ITEMS.len() + 1) % ACTIONS.len(),
                VK_RETURN if self.at < ITEMS.len() => self.change(self.at, 1),
                VK_RETURN => match self.at - ITEMS.len() {
                    0 => {
                        if self.save() {
                            self.restart(false);
                        }
                    }
                    1 => {
                        self.save();
                    }
                    PRELOAD if !confirmed => self.note = CONFIRM_PRELOAD.to_owned(),
                    PRELOAD => {
                        if self.save() {
                            self.restart(true);
                        }
                    }
                    3 => self.reset(),
                    _ => crate::SETTINGS_MENU_OPEN.store(false, Ordering::Relaxed),
                },
                _ => {}
            }
            self.changed = true;
        }
    }

    /// Records the copy of the box onto `back` (in the copy-destination state).
    pub(super) fn draw(&mut self, host: &Host, list: &HostList, back: &ID3D12Resource, (width, height): (u32, u32), format: DXGI_FORMAT) {
        if self.failed {
            return;
        }
        // Closed: the frame-rate line in the top left corner, if it is wanted and there is a rate.
        let open = Self::open();
        let line = if open { None } else { fps_line() };
        if !open && line.is_none() {
            return;
        }
        // Our own copies are not part of a recorded frame (the recorder is in diagnostics builds).
        #[cfg(feature = "diagnostics")]
        let _ours = crate::replay::record::unrecorded();
        let scale = (height / 540).max(1);
        // The menu's scale: 1 at 1080 lines, smaller where the picture is narrow.
        let s = (height as f32 / 1080.0).min(width as f32 / 1240.0).max(0.5);
        let menu = look::size(s);
        let held = (menu.0.max(size(scale).0), menu.1.max(size(scale).1));
        if self.buffers.as_ref().is_none_or(|b| (b.format, b.size) != (format, held)) {
            std::mem::forget(self.buffers.take());
            self.buffers = Buffers::new(host, format, held);
            self.failed = self.buffers.is_none();
            self.changed = true;
        }
        if self.painted != line {
            self.painted = line.clone();
            self.changed = true;
        }
        let Some(colours) = colours(format) else { return };
        let bytes = colours[0].len();
        let picture = if !self.changed {
            None
        } else if let Some(l) = &line {
            let pitch = self.buffers.as_ref().map_or(0, |b| b.pitch as usize);
            Some(painted(&[l.clone()], scale, colours, pitch, held.1 as usize))
        } else {
            let pitch = self.buffers.as_ref().map_or(0, |b| b.pitch as usize);
            self.picture(s).map(|pixels| converted(&pixels, menu.0 as usize, format, bytes, pitch, held.1 as usize))
        };
        let Some(buffers) = &mut self.buffers else { return };
        if self.changed {
            self.changed = false;
            if let Some(picture) = picture {
                buffers.write(picture);
            }
        }
        // The whole box in the middle, or of it the first line's part in the corner.
        let (w, h) = match &line {
            Some(l) => ((l.len() as u32 * CELL.0 + 2 * RIM) * scale, (CELL.1 + 2 * RIM) * scale),
            None => menu,
        };
        let (w, h) = (w.min(width), h.min(height));
        let (x, y) = if line.is_some() { (0, 0) } else { ((width - w) / 2, (height - h) / 2) };
        let list = &list.list;
        // SAFETY: an open list and live resources; the box lies within both.
        unsafe {
            if std::mem::take(&mut buffers.fresh) {
                if std::mem::replace(&mut buffers.used, true) {
                    list.ResourceBarrier(&[transition(&buffers.card, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_COPY_DEST)]);
                }
                list.CopyResource(&buffers.card, &buffers.uploads[buffers.turn].0);
                list.ResourceBarrier(&[transition(&buffers.card, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
            }
            let from = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&buffers.card),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT { Format: format, Width: buffers.size.0, Height: buffers.size.1, Depth: 1, RowPitch: buffers.pitch },
                    },
                },
            };
            let to = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(back),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
            };
            let part = D3D12_BOX { left: 0, top: 0, front: 0, right: w, bottom: h, back: 1 };
            list.CopyTextureRegion(&to, x, y, 0, &from, Some(&part));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_fits_and_every_character_has_a_picture() {
        let mut menu = Menu { values: ITEMS.iter().map(|_| String::new()).collect(), started: Vec::new(), at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false, tab: 0 };
        menu.started = menu.values.clone();
        for at in 0..ITEMS.len() + ACTIONS.len() {
            menu.at = at;
            for line in menu.lines() {
                assert!(line.len() <= COLUMNS as usize, "{line}");
                for c in line.bytes() {
                    assert_ne!(glyph(c.to_ascii_uppercase()), glyph(b'#'), "{line}: {}", c as char);
                }
            }
        }
    }

    #[test]
    fn the_link_is_always_right_under_the_note() {
        let mut menu = Menu { values: ITEMS.iter().map(|_| String::new()).collect(), started: Vec::new(), at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false, tab: 0 };
        menu.started = menu.values.clone();
        for note in ["", "SAVED"] {
            menu.note = note.to_owned();
            for at in 0..ITEMS.len() + ACTIONS.len() {
                menu.at = at;
                let lines = menu.lines();
                assert_eq!(lines.len(), LINES as usize);
                let n = lines.len();
                assert!(lines[n - 2].starts_with("* = CHANGED") || lines[n - 2] == note, "{}", lines[n - 2]);
                assert_eq!(lines[n - 1], REPO);
                // What a preload costs, and what MFG Unlocker is, is said while it is picked, and only then.
                let about = &lines[n - 5..n - 2];
                if at == ITEMS.len() + PRELOAD {
                    assert_eq!(about, ABOUT_PRELOAD);
                } else if at < ITEMS.len() && ITEMS[at].var == MFG {
                    assert_eq!(about, ABOUT_MFG);
                } else {
                    assert!(about.iter().all(String::is_empty), "{about:?}");
                }
            }
        }
        assert_eq!(ACTIONS[PRELOAD], "PRELOAD SHADERS (RESTARTS THE GAME)");
        assert!(CONFIRM_PRELOAD.len() <= COLUMNS as usize);
        assert!(ABOUT_MFG.iter().all(|l| l.len() <= COLUMNS as usize));
        assert_eq!(ITEMS.iter().filter(|i| i.var == MFG).count(), 1);
    }

    #[test]
    fn the_picture_has_tabs_rows_and_a_fading_bar() {
        let mut menu = Menu { values: ITEMS.iter().map(|i| i.values[0].0.to_owned()).collect(), started: Vec::new(), at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false, tab: 0 };
        menu.started = menu.values.clone();
        menu.to_tab(1);
        let (w, h) = look::size(1.0);
        let pixels = menu.picture(1.0).expect("GDI draws");
        assert_eq!(pixels.len(), (w * h) as usize);
        let at = |x: u32, y: u32| pixels[(y * w + x) as usize];
        // The picked row's bar: white at its left, fading to the right.
        let row_y = 138 + 23;
        let left = at(30, row_y);
        let right = at(750, row_y);
        assert!(left & 0xFF > 0xF0, "{left:x}");
        assert!(right & 0xFF < left & 0xFF, "{right:x}");
        // Every tab has rows, and stepping goes through them and onto the actions.
        for tab in 0..TABS.len() {
            menu.to_tab(tab);
            assert!(menu.at < ITEMS.len());
            for _ in 0..tab_items(tab).len() {
                menu.step(1);
            }
            assert_eq!(menu.at, ITEMS.len());
        }
        assert_eq!(pretty("FRAME RATE CAP"), "Frame Rate Cap");
        assert_eq!(pretty("3440X1440 (21:9)"), "3440x1440 (21:9)");
        assert_eq!(pretty("DLSS MODEL"), "DLSS Model");
        assert_eq!(half(1.0), 0x3C00);
    }

    /// `VICTORY_MENU_PREVIEW=<file.bmp>`: the picture of each tab written out, to look at without the game.
    #[test]
    fn preview() {
        let Some(path) = std::env::var_os("VICTORY_MENU_PREVIEW") else { return };
        let mut menu = Menu { values: ITEMS.iter().map(|i| i.values[0].0.to_owned()).collect(), started: Vec::new(), at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false, tab: 0 };
        menu.started = menu.values.clone();
        menu.values[tab_items(1)[1]] = ITEMS[tab_items(1)[1]].values[1].0.to_owned();
        for tab in 0..TABS.len() {
            menu.to_tab(tab);
            menu.step(1);
            let (w, h) = look::size(1.0);
            let pixels = menu.picture(1.0).unwrap();
            let mut bmp = Vec::new();
            let size = 54 + pixels.len() as u32 * 4;
            bmp.extend(b"BM");
            for v in [size, 0, 54, 40, w, (h as i32).wrapping_neg() as u32] {
                bmp.extend(v.to_le_bytes());
            }
            bmp.extend(1u16.to_le_bytes());
            bmp.extend(32u16.to_le_bytes());
            bmp.extend([0u8; 24]);
            for p in &pixels {
                bmp.extend(p.to_le_bytes());
            }
            let mut out = std::path::PathBuf::from(&path);
            out.set_extension(format!("{tab}.bmp"));
            std::fs::write(out, bmp).unwrap();
        }
    }

    #[test]
    fn values_cycle_both_ways() {
        let mut menu = Menu { values: ITEMS.iter().map(|i| i.values[0].0.to_owned()).collect(), started: Vec::new(), at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false, tab: 0 };
        menu.started = menu.values.clone();
        let n = ITEMS[0].values.len();
        for _ in 0..n {
            menu.change(0, 1);
        }
        assert_eq!(menu.values[0], ITEMS[0].values[0].0);
        menu.change(0, -1);
        assert_eq!(menu.values[0], ITEMS[0].values[n - 1].0);
    }
}
