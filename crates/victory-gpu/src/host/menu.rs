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
    GetAsyncKeyState, VK_DOWN, VK_ESCAPE, VK_F1, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::font::{ASCII_WIDTH, TextBox, colours, supports_chinese, width as text_width};
use super::{Host, HostList, transition};

const COLUMNS: u32 = 60;
/// Lines of the box: a title, a help line, a blank, the settings shown, a
/// blank, what the line picked does (`ABOUT`, else blank), a note and the
/// repository's link.
const ROWS: usize = 20;
const LINES: u32 = ROWS as u32 + 9;
/// The action that preloads shaders, and what is said of it while it is picked.
const PRELOAD: usize = 2;
const ABOUT_PRELOAD: [Labels; 3] = [
    ("MAKES EVERY SHADER AHEAD, THEN STARTS THE GAME AGAIN.", "提前编译全部着色器，然后重新启动游戏。"),
    ("TAKES A WHILE: MAYBE A LONG TIME, DEPENDING ON HARDWARE.", "处理时间取决于硬件，可能需要等很久。"),
    ("A LATER UPDATE OF THIS BUILD MAY NEED A NEW PRELOAD.", "兼容层更新后可能需要重新预加载。"),
];
/// The note after the first Enter on it: the second one does it.
const CONFIRM_PRELOAD: Labels = ("ENTER AGAIN: SAVE, CLOSE THE GAME AND PRELOAD", "再次按回车：保存设置、退出游戏并预加载");
/// The last line of the box: where this build comes from.
const REPO: &str = "GITHUB.COM/TRUSTNO1-CODER/VICTORY-NVIDIA";
const BOX: TextBox = TextBox { columns: COLUMNS, lines: LINES, rim: 4 };
const FPS_BOX: TextBox = TextBox { columns: 38, lines: 1, rim: 4 };
const UPLOADS: usize = 4;

const LANGUAGE: &str = "VICTORY_MENU_LANGUAGE";
type Labels = (&'static str, &'static str);

fn label((english, chinese): Labels, use_chinese: bool) -> &'static str {
    if use_chinese { chinese } else { english }
}

const fn same(value: &'static str) -> Labels {
    (value, value)
}

fn chinese_requested() -> &'static AtomicBool {
    static CHINESE: OnceLock<AtomicBool> = OnceLock::new();
    CHINESE.get_or_init(|| AtomicBool::new(get(LANGUAGE).eq_ignore_ascii_case("zh-CN")))
}

pub(super) fn is_chinese() -> bool {
    chinese_requested().load(Ordering::Relaxed) && supports_chinese()
}

/// A setting: the variable, its name in the menu, the values it takes
/// (`""`: not set, the build's own default), and whether a change applies at once.
struct Item {
    var: &'static str,
    label: Labels,
    values: &'static [(&'static str, Labels)],
    live: bool,
}

const ON: Labels = ("ON", "开启");
const OFF: Labels = ("OFF", "关闭");
const NONE: Labels = ("NONE", "无限制");
const ON_UNSET: &[(&str, Labels)] = &[("1", ON), ("", OFF)];
const UNSET_ON: &[(&str, Labels)] = &[("", OFF), ("1", ON)];
const STAGES: &[(&str, Labels)] = &[("2", same("2")), ("1", same("1")), ("0", ("0 OFF", "0 关闭"))];

/// The first value of each is what the launcher starts with.
#[rustfmt::skip]
const ITEMS: &[Item] = &[
    Item { var: LANGUAGE, label: ("LANGUAGE", "界面语言"), values: &[("en", same("ENGLISH")), ("zh-CN", ("SIMPLIFIED CHINESE", "简体中文"))], live: true },
    // The launcher's own choices.
    Item { var: "VICTORY_MENU_DLSS", label: same("DLSS"), values: &[("quality", ("QUALITY", "质量")), ("balanced", ("BALANCED", "平衡")), ("performance", ("PERFORMANCE", "性能")), ("dlaa", ("DLAA (NATIVE)", "DLAA (原生分辨率)")), ("off", OFF)], live: false },
    Item { var: "VICTORY_MENU_FRAMEGEN", label: ("FRAME GENERATION", "帧生成"), values: &[("0", OFF), ("2", same("2X")), ("3", same("3X")), ("4", same("4X")), ("5", same("5X")), ("6", ("6X (MOST)", "6X (最高)"))], live: false },
    // DLSS Frame Generation where it runs (RTX 40 and later), else AMD's FSR 3 (2X only).
    Item { var: "VICTORY_MENU_FG_TYPE", label: ("FRAME GEN TYPE", "帧生成类型"), values: &[("auto", ("AUTO", "自动")), ("dlss", same("DLSS")), ("fsr", ("FSR 3 (OLDER CARDS)", "FSR 3 (旧显卡)"))], live: false },
    Item { var: "VICTORY_MENU_FPS", label: ("FRAME RATE CAP", "帧率上限"), values: &[("1000", NONE), ("120", same("120")), ("60", same("60")), ("30", ("30 (THE GAME)", "30 (游戏默认)"))], live: false },
    Item { var: "VICTORY_FPS_LINE", label: ("FPS AT THE TOP", "顶部帧率"), values: &[("1", ON), ("0", OFF)], live: true },
    Item { var: "VICTORY_PANEL", label: ("STATS PANEL (F3)", "性能统计 (F3)"), values: &[("0", OFF), ("1", ON)], live: true },
    Item { var: "VICTORY_FULLSCREEN", label: ("FULLSCREEN (NO BORDER)", "无边框全屏"), values: UNSET_ON, live: false },
    Item { var: "VICTORY_MOUSE_CAPTURE", label: ("MOUSE CAPTURE", "锁定鼠标"), values: UNSET_ON, live: false },
    Item { var: "VICTORY_IME_ENGLISH", label: ("ENGLISH KEYBOARD", "使用英文键盘"), values: UNSET_ON, live: false },
    Item { var: "VICTORY_MENU_CONSOLE", label: ("CONSOLE", "主机型号"), values: &[("series-x", same("SERIES X")), ("series-s", ("SERIES S (FASTER)", "SERIES S (更快)"))], live: false },
    Item { var: "VICTORY_MENU_RES", label: ("SERIES X PICTURE", "SERIES X 分辨率"), values: &[("1440", same("2560X1440")), ("1080to1440", ("1080P TO 1440P", "1080P 升至 1440P")), ("4k", same("3840X2160"))], live: false },
    // The layer's switches.
    Item { var: "VICTORY_DLSS_PRESET", label: ("DLSS MODEL", "DLSS 模型"), values: &[("", ("DEFAULT", "默认")), ("k", same("K")), ("j", same("J")), ("m", same("M")), ("l", same("L"))], live: false },
    Item { var: "VICTORY_AMBIENT", label: ("LIGHT ESTIMATE", "环境光估算"), values: &[("0.8", same("0.8")), ("1", same("1")), ("0.5", same("0.5")), ("", OFF)], live: false },
    Item { var: "VICTORY_AMBIENT_SPECULAR", label: ("REFLECTION ESTIMATE", "反射估算"), values: ON_UNSET, live: false },
    Item { var: "VICTORY_AMBIENT_AIR", label: ("FOG CAP", "雾效强度上限"), values: &[("1.5", same("1.5")), ("1", same("1")), ("2", same("2")), ("", NONE)], live: false },
    Item { var: "VICTORY_BVH_BUILDER", label: ("RAY TRACING (SLOW, BROKEN)", "光线追踪 (慢且有故障)"), values: UNSET_ON, live: false },
    Item { var: "VICTORY_WATER_LIGHT", label: ("WATER LIGHT", "水面光照"), values: &[("", ("OFF (NO DARK BLOCKS)", "关闭 (无黑块)")), ("1", ("ON (GREEN, DARK BLOCKS)", "开启 (偏绿和黑块)"))], live: false },
    Item { var: "VICTORY_LEAVE_OUT", label: ("HEAT HAZE AND DECAL DEFRAG", "热浪和贴花整理"), values: &[("02384a60fda6f2d0,731a39171b568298", ("LEFT OUT", "跳过")), ("", ("RUN (SLOWER, SMEARS)", "开启 (慢且有拖影)"))], live: false },
    Item { var: "VICTORY_EARLY_DEPTH", label: ("EARLY DEPTH TEST", "提前深度测试"), values: ON_UNSET, live: false },
    Item { var: "VICTORY_WC_NO_BACK", label: ("SKIP BIG COPY-BACKS", "跳过大块数据回拷"), values: ON_UNSET, live: false },
    Item { var: "VICTORY_SOFT_VIEWS", label: ("SOFT VIEWS (NO CUT FREEZES)", "软视图 (减少转场卡顿)"), values: &[("copies", ON), ("", OFF)], live: false },
    Item { var: "VICTORY_TEXTURE_NO_WAIT", label: ("TEXTURE NO WAIT", "纹理异步加载"), values: ON_UNSET, live: false },
    Item { var: "VICTORY_RELAY", label: ("RELAY (SPEED, TRIANGLES)", "GPU 中继 (速度、三角面)"), values: STAGES, live: false },
    Item { var: "VICTORY_CARD_STEPS", label: ("CARD STEPS", "GPU 处理阶段"), values: STAGES, live: false },
    Item { var: "VICTORY_INDIRECT_BATCH", label: ("INDIRECT BATCH", "间接绘制批处理"), values: STAGES, live: false },
    Item { var: "VICTORY_CARD_BUFFERS_MIN_KB", label: ("CARD COMPARES FROM (KB)", "GPU 缓冲区比较阈值 (KB)"), values: &[("64", same("64")), ("1024", same("1024"))], live: false },
    Item { var: "VICTORY_PORT_CONSTANTS", label: ("CONSTANTS HEAP IN PLACE", "原地更新常量缓冲区"), values: UNSET_ON, live: false },
];
/// The lines after the settings.
const ACTIONS: [Labels; 5] = [
    ("SAVE AND RESTART NOW", "保存并立即重启"),
    ("SAVE FOR NEXT START", "保存设置"),
    ("PRELOAD SHADERS (RESTARTS THE GAME)", "预加载着色器 (重启游戏)"),
    ("RESET ALL TO PLAY DEFAULTS", "恢复启动默认设置"),
    ("CLOSE", "关闭"),
];

fn get(var: &str) -> String {
    std::env::var(var).unwrap_or_default()
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
fn fps_line(chinese: bool) -> Option<String> {
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
    let rate = label(("FPS", "帧率"), chinese);
    let per = crate::streamline::shown_per_frame();
    Some(if per == 1 {
        format!("{rate}: {fps:.1}")
    } else {
        let displayed = label(("FG", "显示帧率"), chinese);
        format!("{rate}: {fps:.1}  {displayed}: {:.1}", fps * f64::from(per))
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
fn shown_value(item: &Item, value: &str, chinese: bool) -> String {
    match index_of(item, value) {
        Some(i) => label(item.values[i].1, chinese).to_owned(),
        None if value.is_empty() => label(("NOT SET", "未设置"), chinese).to_owned(),
        None => value.to_ascii_uppercase(),
    }
}

/// Keys pressed, gathered on a thread of their own every 10 ms: a frame
/// lasts 50 ms and more, and a short press would fall between two frames.
fn keys() -> &'static Mutex<Vec<u16>> {
    static KEYS: OnceLock<Mutex<Vec<u16>>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let _ = std::thread::Builder::new().name("settings menu keys".into()).spawn(|| {
            const WATCHED: [u16; 7] = [VK_F1, VK_UP, VK_DOWN, VK_LEFT, VK_RIGHT, VK_RETURN, VK_ESCAPE];
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

struct Buffers {
    shape: TextBox,
    format: DXGI_FORMAT,
    scale: u32,
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
    fn new(host: &Host, shape: TextBox, format: DXGI_FORMAT, chinese: bool, scale: u32) -> Option<Buffers> {
        let (width, height) = shape.size(chinese, scale);
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
        Some(Buffers { shape, format, scale, pitch, uploads, card: card?, turn: 0, fresh: false, used: false })
    }

    fn write(&mut self, lines: &[String], chinese: bool) {
        let Some(colours) = colours(self.format) else { return };
        let picture = self.shape.paint(lines, chinese, self.scale, colours, self.pitch as usize);
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
    note: Option<Labels>,
    changed: bool,
    /// What the buffer holds: the frame-rate line, or (none) the menu's lines.
    painted: Option<String>,
    buffers: [Option<Buffers>; 2],
    fps_buffers: [Option<Buffers>; 2],
    failed: bool,
}

impl Menu {
    pub(super) fn new() -> Menu {
        // Start the key thread now: F1 must be seen while the menu is closed.
        let _ = keys();
        // `VICTORY_MENU_OPEN=1`: shown from the start (to look at it in a run nobody presses keys in).
        if std::env::var_os("VICTORY_MENU_OPEN").is_some_and(|v| v == "1") {
            crate::SETTINGS_MENU_OPEN.store(true, Ordering::Relaxed);
        }
        let values: Vec<String> = ITEMS.iter().map(|i| {
            if i.var == LANGUAGE {
                label(("en", "zh-CN"), chinese_requested().load(Ordering::Relaxed)).to_owned()
            } else {
                get(i.var)
            }
        }).collect();
        Menu { started: values.clone(), values, at: 0, note: None, changed: true, painted: None, buffers: [None, None], fps_buffers: [None, None], failed: false }
    }

    fn open() -> bool {
        crate::SETTINGS_MENU_OPEN.load(Ordering::Relaxed)
    }

    fn lines(&self, chinese: bool) -> Vec<String> {
        let tr = |labels| label(labels, chinese);
        let total = ITEMS.len() + ACTIONS.len();
        let first = self.at.saturating_sub(ROWS / 2).min(total.saturating_sub(ROWS));
        let mut out = vec![
            tr(("SETTINGS                       F1 OR ESC CLOSES", "设置                     F1 / ESC 关闭")).to_owned(),
            tr(("UP/DOWN PICK  LEFT/RIGHT CHANGE  ENTER DOES IT", "上下选择    左右修改    回车确认")).to_owned(),
            String::new(),
        ];
        for row in first..(first + ROWS).min(total) {
            let cursor = if row == self.at { '>' } else { ' ' };
            let line = if let Some(item) = ITEMS.get(row) {
                let value = &self.values[row];
                let shown = shown_value(item, value, chinese);
                let mark = match (*value != self.started[row], live(item)) {
                    (true, true) => tr((" *LIVE", " *即时")),
                    (true, false) => " *",
                    (false, true) => tr((" LIVE", " 即时")),
                    (false, false) => "",
                };
                let name = tr(item.label);
                let padding = (27 * ASCII_WIDTH).saturating_sub(text_width(name)) / ASCII_WIDTH;
                format!("{cursor} {name}{:width$} < {shown} >{mark}", "", width = padding as usize)
            } else {
                format!("{cursor} {}", tr(ACTIONS[row - ITEMS.len()]))
            };
            out.push(line);
        }
        while out.len() < 3 + ROWS {
            out.push(String::new());
        }
        out.push(String::new());
        let about = if self.at == ITEMS.len() + PRELOAD { ABOUT_PRELOAD } else { [same(""); 3] };
        out.extend(about.map(|line| tr(line).to_owned()));
        let note = self.note.or_else(|| {
            (!chinese && chinese_requested().load(Ordering::Relaxed)).then_some(same("CHINESE FONTS NOT FOUND IN WINDOWS"))
        });
        out.push(tr(note.unwrap_or((
            "* = CHANGED: APPLIES AT THE NEXT START UNLESS LIVE",
            "* 已修改：即时项立即生效，其余下次启动生效",
        ))).to_owned());
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
        self.note = None;
    }

    /// A setting the build reads at each use: changed in this run's environment at once.
    fn apply_live(&self, i: usize) {
        let (var, value) = (ITEMS[i].var, &self.values[i]);
        if var == LANGUAGE {
            chinese_requested().store(value == "zh-CN", Ordering::Relaxed);
            return;
        }
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
            self.note = Some(("NO SETTINGS FILE: START WITH VICTORY PLAY", "找不到设置文件，请通过 VICTORY PLAY 启动"));
            return false;
        };
        let mut kept: Vec<(String, String)> = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())))
            .filter(|(k, _)| k.starts_with("VICTORY_"))
            .collect();
        for (i, item) in ITEMS.iter().enumerate() {
            // The live language can outlast a presenter recreated before Save.
            if self.values[i] == self.started[i] && item.var != LANGUAGE {
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
                self.note = Some(("SAVED. APPLIES AT THE NEXT START", "已保存。其余设置在下次启动时生效"));
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "settings menu: not saved");
                self.note = Some(("COULD NOT SAVE THE SETTINGS FILE", "无法写入设置文件"));
                false
            }
        }
    }

    /// Starts the launcher again in a new console, a few seconds on, and ends this run.
    /// `preload`: the launcher makes every shader first (its `-Preload`, for that start alone).
    fn restart(&mut self, preload: bool) {
        let Some(play) = std::env::var_os("VICTORY_MENU_PLAY") else {
            self.note = Some(("SAVED. START THE LAUNCHER AGAIN YOURSELF", "已保存。请手动重新运行启动器"));
            return;
        };
        let play = play.to_string_lossy().replace('\'', "''");
        let args = get("VICTORY_MENU_ARGS");
        let command = format!("Start-Sleep 3; & '{play}' {args}{}", if preload { " -Preload" } else { "" });
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
                self.note = Some(("SAVED, BUT THE RESTART FAILED", "已保存，重启失败"));
            }
        }
    }

    /// Removes the settings file: the launcher's own settings at the next start.
    fn reset(&mut self) {
        if let Some(path) = std::env::var_os("VICTORY_MENU_FILE") {
            let _ = std::fs::remove_file(path);
        }
        self.note = Some(("RESET. PLAY DEFAULTS AT THE NEXT START", "已重置。下次启动将使用默认设置"));
    }

    /// A frame was presented: the keys pressed since the last one.
    pub(super) fn frame(&mut self) {
        let pressed = std::mem::take(&mut *keys().lock().unwrap_or_else(|e| e.into_inner()));
        if pressed.is_empty() {
            return;
        }
        let total = ITEMS.len() + ACTIONS.len();
        for vk in pressed {
            let open = Self::open();
            if vk == VK_F1 || (open && vk == VK_ESCAPE) {
                crate::SETTINGS_MENU_OPEN.store(!open && vk == VK_F1, Ordering::Relaxed);
                self.note = None;
                self.changed = true;
                continue;
            }
            if !open {
                continue;
            }
            // Only an Enter right after the first one starts a preload.
            let confirmed = self.note == Some(CONFIRM_PRELOAD);
            if confirmed {
                self.note = None;
            }
            match vk {
                VK_UP => self.at = (self.at + total - 1) % total,
                VK_DOWN => self.at = (self.at + 1) % total,
                VK_LEFT if self.at < ITEMS.len() => self.change(self.at, -1),
                VK_RIGHT if self.at < ITEMS.len() => self.change(self.at, 1),
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
                    PRELOAD if !confirmed => self.note = Some(CONFIRM_PRELOAD),
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
        let chinese = is_chinese();
        let line = if open { None } else { fps_line(chinese) };
        if !open && line.is_none() {
            return;
        }
        // Our own copies are not part of a recorded frame (the recorder is in diagnostics builds).
        #[cfg(feature = "diagnostics")]
        let _ours = crate::replay::record::unrecorded();
        let scale = (height / 540).max(1);
        let shape = if open { BOX } else { FPS_BOX };
        let mode = usize::from(chinese);
        let storage = if open { &mut self.buffers } else { &mut self.fps_buffers };
        let buffers = &mut storage[mode];
        if buffers.as_ref().is_none_or(|b| (b.format, b.scale) != (format, scale)) {
            std::mem::forget(buffers.take());
            *buffers = Buffers::new(host, shape, format, chinese, scale);
            self.failed = buffers.is_none();
            self.changed = true;
        }
        if self.painted != line {
            self.painted = line.clone();
            self.changed = true;
        }
        let lines = if self.changed { Some(line.as_ref().map_or_else(|| self.lines(chinese), |l| vec![l.clone()])) } else { None };
        let storage = if open { &mut self.buffers } else { &mut self.fps_buffers };
        let Some(buffers) = &mut storage[mode] else { return };
        if let Some(lines) = lines {
            self.changed = false;
            buffers.write(&lines, chinese);
        }
        // The whole box in the middle, or of it the first line's part in the corner.
        let (box_width, box_height) = shape.size(chinese, scale);
        let (w, h) = line.as_ref().map_or((box_width, box_height), |l| {
            shape.area(text_width(l).min(shape.columns * ASCII_WIDTH), 1, chinese, scale)
        });
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
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT { Format: format, Width: box_width, Height: box_height, Depth: 1, RowPitch: buffers.pitch },
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
    use crate::host::font::glyph;

    #[test]
    fn every_line_fits_and_every_character_has_a_picture() {
        let mut menu = Menu { values: ITEMS.iter().map(|_| String::new()).collect(), started: Vec::new(), at: 0, note: None, changed: false, painted: None, buffers: [None, None], fps_buffers: [None, None], failed: false };
        menu.started = menu.values.clone();
        for chinese in [false, true] {
            for (at, item) in ITEMS.iter().enumerate() {
                menu.at = at;
                for (value, _) in item.values {
                    menu.values[at] = (*value).to_owned();
                    for line in menu.lines(chinese) {
                        assert!(text_width(&line) <= COLUMNS * ASCII_WIDTH, "{line}");
                        for c in line.bytes().filter(u8::is_ascii) {
                            assert_ne!(glyph(c.to_ascii_uppercase()), glyph(b'#'), "{line}: {}", c as char);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_link_is_always_right_under_the_note() {
        let mut menu = Menu { values: ITEMS.iter().map(|_| String::new()).collect(), started: Vec::new(), at: 0, note: None, changed: false, painted: None, buffers: [None, None], fps_buffers: [None, None], failed: false };
        menu.started = menu.values.clone();
        for note in [None, Some(same("SAVED"))] {
            menu.note = note;
            for at in 0..ITEMS.len() + ACTIONS.len() {
                menu.at = at;
                let lines = menu.lines(false);
                assert_eq!(lines.len(), LINES as usize);
                let n = lines.len();
                assert!(lines[n - 2].starts_with("* = CHANGED") || note.is_some_and(|note| lines[n - 2] == note.0), "{}", lines[n - 2]);
                assert_eq!(lines[n - 1], REPO);
                // What a preload costs is said while it is picked, and only then.
                let about = &lines[n - 5..n - 2];
                if at == ITEMS.len() + PRELOAD {
                    assert_eq!(about, ABOUT_PRELOAD.map(|line| line.0));
                } else {
                    assert!(about.iter().all(String::is_empty), "{about:?}");
                }
            }
        }
        assert_eq!(ACTIONS[PRELOAD].0, "PRELOAD SHADERS (RESTARTS THE GAME)");
        assert!(text_width(CONFIRM_PRELOAD.0) <= COLUMNS * ASCII_WIDTH);
        assert!(text_width(CONFIRM_PRELOAD.1) <= COLUMNS * ASCII_WIDTH);
    }

    #[test]
    fn values_cycle_both_ways() {
        let mut menu = Menu { values: ITEMS.iter().map(|i| i.values[0].0.to_owned()).collect(), started: Vec::new(), at: 0, note: None, changed: false, painted: None, buffers: [None, None], fps_buffers: [None, None], failed: false };
        menu.started = menu.values.clone();
        let i = 1; // DLSS, a setting with more than two options.
        let n = ITEMS[i].values.len();
        for _ in 0..n {
            menu.change(i, 1);
        }
        assert_eq!(menu.values[i], ITEMS[i].values[0].0);
        menu.change(i, -1);
        assert_eq!(menu.values[i], ITEMS[i].values[n - 1].0);
    }
}
