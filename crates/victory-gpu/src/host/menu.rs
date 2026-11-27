//! The F1 settings menu (Chinese). F1 shows and hides it while our window has
//! the focus; Up/Down pick a setting, Left/Right change it, Enter runs the line
//! picked (save, save and restart, reset, close), Esc closes. While it is
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
//! Text is drawn with Windows GDI in Microsoft YaHei so Chinese labels work;
//! the picture is copied onto the swap chain's buffer, in the middle of the
//! screen. The stats panel (F3) keeps the dot font of `font.rs`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_DOWN, VK_ESCAPE, VK_F1, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::font::colours;
use super::{Host, HostList, transition};

/// Box layout in base units, multiplied by the picture's scale.
const MENU_W: u32 = 470;
const ROW_H: u32 = 11;
/// Lines of the box: a title, a help line, a blank, the settings shown, a
/// blank and a note.
const ROWS: usize = 20;
const LINES: u32 = ROWS as u32 + 5;
const RIM: u32 = 4;
const LABEL_X: u32 = 14;
const VALUE_X: u32 = 252;
const UPLOADS: usize = 4;

/// A setting: the variable, its name in the menu, the values it takes
/// (`""`: not set, the build's own default), and whether a change applies at once.
struct Item {
    var: &'static str,
    label: &'static str,
    values: &'static [(&'static str, &'static str)],
    live: bool,
}

const ON_UNSET: &[(&str, &str)] = &[("1", "开"), ("", "关")];
const UNSET_ON: &[(&str, &str)] = &[("", "关"), ("1", "开")];
const STAGES: &[(&str, &str)] = &[("2", "2"), ("1", "1"), ("0", "0 关")];

/// The first value of each is what the launcher starts with.
#[rustfmt::skip]
const ITEMS: &[Item] = &[
    // The launcher's own choices.
    Item { var: "VICTORY_MENU_DLSS", label: "DLSS 超分辨率", values: &[("quality", "质量"), ("balanced", "平衡"), ("performance", "性能"), ("dlaa", "DLAA（原生）"), ("off", "关闭")], live: false },
    Item { var: "VICTORY_MENU_FRAMEGEN", label: "帧生成", values: &[("0", "关闭"), ("2", "2 倍"), ("3", "3 倍"), ("4", "4 倍"), ("5", "5 倍"), ("6", "6 倍（最高）")], live: false },
    Item { var: "VICTORY_MENU_FPS", label: "帧率上限", values: &[("1000", "无限制"), ("120", "120"), ("60", "60"), ("30", "30（游戏原生）")], live: false },
    Item { var: "VICTORY_FPS_LINE", label: "左上角帧率显示", values: &[("1", "开"), ("0", "关")], live: true },
    Item { var: "VICTORY_PANEL", label: "性能面板（F3）", values: &[("0", "关"), ("1", "开")], live: true },
    Item { var: "VICTORY_FULLSCREEN", label: "无边框全屏", values: UNSET_ON, live: false },
    Item { var: "VICTORY_MOUSE_CAPTURE", label: "鼠标锁定", values: UNSET_ON, live: false },
    Item { var: "VICTORY_IME_ENGLISH", label: "英文键盘布局", values: UNSET_ON, live: false },
    Item { var: "VICTORY_MENU_CONSOLE", label: "主机配置", values: &[("series-x", "Series X"), ("series-s", "Series S（更快）")], live: false },
    Item { var: "VICTORY_MENU_RES", label: "画面分辨率", values: &[("1440", "2560×1440"), ("1080to1440", "1080P → 1440P"), ("4k", "3840×2160")], live: false },
    // The layer's switches.
    Item { var: "VICTORY_DLSS_PRESET", label: "DLSS 模型", values: &[("", "默认"), ("k", "K"), ("j", "J"), ("m", "M"), ("l", "L")], live: false },
    Item { var: "VICTORY_AMBIENT", label: "间接光估算", values: &[("0.8", "0.8"), ("1", "1"), ("0.5", "0.5"), ("", "关闭")], live: false },
    Item { var: "VICTORY_AMBIENT_SPECULAR", label: "反射估算", values: ON_UNSET, live: false },
    Item { var: "VICTORY_AMBIENT_AIR", label: "雾效上限", values: &[("1.5", "1.5"), ("1", "1"), ("2", "2"), ("", "无")], live: false },
    Item { var: "VICTORY_BVH_BUILDER", label: "光线追踪（慢且异常）", values: UNSET_ON, live: false },
    Item { var: "VICTORY_WATER_LIGHT", label: "水面光照", values: &[("", "关闭（无黑块）"), ("1", "开启（偏绿、有黑块）")], live: false },
    Item { var: "VICTORY_LEAVE_OUT", label: "热浪与贴花整理", values: &[("02384a60fda6f2d0,731a39171b568298", "跳过"), ("", "运行（慢、拖影）")], live: false },
    Item { var: "VICTORY_EARLY_DEPTH", label: "早期深度测试", values: ON_UNSET, live: false },
    Item { var: "VICTORY_WC_NO_BACK", label: "跳过大块回拷", values: ON_UNSET, live: false },
    Item { var: "VICTORY_SOFT_VIEWS", label: "软视图（防切场景卡顿）", values: &[("copies", "开"), ("", "关")], live: false },
    Item { var: "VICTORY_TEXTURE_NO_WAIT", label: "纹理不等待", values: ON_UNSET, live: false },
    Item { var: "VICTORY_RELAY", label: "中继（速度/三角形）", values: STAGES, live: false },
    Item { var: "VICTORY_CARD_STEPS", label: "显卡步数", values: STAGES, live: false },
    Item { var: "VICTORY_INDIRECT_BATCH", label: "间接批处理", values: STAGES, live: false },
    Item { var: "VICTORY_CARD_BUFFERS_MIN_KB", label: "显卡比较阈值（KB）", values: &[("64", "64"), ("1024", "1024")], live: false },
    Item { var: "VICTORY_PORT_CONSTANTS", label: "常量堆原位", values: UNSET_ON, live: false },
];
/// The lines after the settings.
const ACTIONS: [&str; 4] = ["立即保存并重启", "保存（下次启动生效）", "全部恢复为启动默认", "关闭"];

/// One drawn line of the box.
enum Row {
    Text(String),
    Item { sel: bool, label: String, value: String, mark: String },
    Action { sel: bool, text: String },
}

fn get(var: &str) -> String {
    std::env::var(var).unwrap_or_default()
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

/// The box's size in pixels.
const fn size(scale: u32) -> (u32, u32) {
    ((MENU_W + 2 * RIM) * scale, (LINES * ROW_H + 2 * RIM) * scale)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// 0..255 as IEEE halves, for the float swap-chain formats.
fn half_table() -> &'static [u16; 256] {
    static T: OnceLock<[u16; 256]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u16; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let v = i as f32 / 255.0;
            let b = v.to_bits();
            let sign = ((b >> 16) & 0x8000) as u16;
            let exp = ((b >> 23) & 0xFF) as i32 - 127 + 15;
            let man = (b >> 13) & 0x3FF;
            *e = if exp >= 31 {
                sign | 0x7C00
            } else if exp <= 0 {
                if exp < -10 { sign } else { sign | (((man | 0x400) >> (1 - exp)) as u16) }
            } else {
                sign | ((exp as u16) << 10) | man as u16
            };
        }
        t
    })
}

/// Draws the rows with GDI (Microsoft YaHei) into a 32-bit DIB and converts
/// the pixels into the swap chain's format. White on black; the picked line
/// is green.
fn render_rows(rows: &[Row], scale: u32, format: DXGI_FORMAT, pitch: usize) -> Option<Vec<u8>> {
    let bytes = colours(format)?[0].len();
    let (w, h) = size(scale);
    let mut out = vec![0u8; pitch * h as usize];
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32);
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0 as u32;
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bmp = match CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(_) => {
                let _ = DeleteDC(hdc);
                return None;
            }
        };
        if bits.is_null() {
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(hdc);
            return None;
        }
        let old = SelectObject(hdc, bmp.into());
        std::ptr::write_bytes(bits as *mut u8, 0, (w * h * 4) as usize);
        let face = wide("Microsoft YaHei UI");
        let font = CreateFontW(
            -(9 * scale as i32), 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY, DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32, PCWSTR::from_raw(face.as_ptr()),
        );
        let font_bold = CreateFontW(
            -(9 * scale as i32), 0, 0, 0, 700, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY, DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32, PCWSTR::from_raw(face.as_ptr()),
        );
        let old_font = SelectObject(hdc, font.into());
        SetBkMode(hdc, TRANSPARENT);
        let white = COLORREF(0x00FFFFFF);
        let green = COLORREF(0x0000B976);
        let mut y = RIM * scale;
        for (i, row) in rows.iter().enumerate() {
            let x0 = (RIM + LABEL_X) * scale;
            match row {
                Row::Text(t) => {
                    if t.is_empty() {
                        y += ROW_H * scale;
                        continue;
                    }
                    SelectObject(hdc, if i == 0 { font_bold.into() } else { font.into() });
                    SetTextColor(hdc, white);
                    let wt = wide(t);
                    let _ = TextOutW(hdc, (RIM * scale) as i32, y as i32, &wt);
                }
                Row::Item { sel, label, value, mark } => {
                    SelectObject(hdc, font.into());
                    SetTextColor(hdc, if *sel { green } else { white });
                    if *sel {
                        let c = wide("▶");
                        let _ = TextOutW(hdc, (RIM * scale) as i32, y as i32, &c);
                    }
                    let wl = wide(label);
                    let _ = TextOutW(hdc, x0 as i32, y as i32, &wl);
                    let shown = format!("< {value} >");
                    let wv = wide(&shown);
                    let vx = (RIM + VALUE_X) * scale;
                    let _ = TextOutW(hdc, vx as i32, y as i32, &wv);
                    if !mark.is_empty() {
                        let mut sz = SIZE::default();
                        let _ = GetTextExtentPoint32W(hdc, &wv, &mut sz);
                        let wm = wide(mark);
                        let _ = TextOutW(hdc, vx as i32 + sz.cx + 4 * scale as i32, y as i32, &wm);
                    }
                }
                Row::Action { sel, text } => {
                    SelectObject(hdc, font.into());
                    SetTextColor(hdc, if *sel { green } else { white });
                    if *sel {
                        let c = wide("▶");
                        let _ = TextOutW(hdc, (RIM * scale) as i32, y as i32, &c);
                    }
                    let wt = wide(text);
                    let _ = TextOutW(hdc, x0 as i32, y as i32, &wt);
                }
            }
            y += ROW_H * scale;
        }
        // DIB bits are BGRA; GDI leaves alpha 0. Convert into the chain's format.
        let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let halves = half_table();
        for py in 0..h as usize {
            for px in 0..w as usize {
                let s = (py * w as usize + px) * 4;
                let (b, g, r) = (src[s] as u32, src[s + 1] as u32, src[s + 2] as u32);
                let at = py * pitch + px * bytes;
                match format {
                    DXGI_FORMAT_B8G8R8A8_UNORM => {
                        out[at..at + 4].copy_from_slice(&[b as u8, g as u8, r as u8, 0xFF]);
                    }
                    DXGI_FORMAT_R8G8B8A8_UNORM => {
                        out[at..at + 4].copy_from_slice(&[r as u8, g as u8, b as u8, 0xFF]);
                    }
                    DXGI_FORMAT_R10G10B10A2_UNORM => {
                        let q = |v: u32| (v << 2) | (v >> 6);
                        let packed = q(r) | (q(g) << 10) | (q(b) << 20) | (3 << 30);
                        out[at..at + 4].copy_from_slice(&packed.to_le_bytes());
                    }
                    DXGI_FORMAT_R16G16B16A16_FLOAT => {
                        out[at..at + 2].copy_from_slice(&halves[r as usize].to_le_bytes());
                        out[at + 2..at + 4].copy_from_slice(&halves[g as usize].to_le_bytes());
                        out[at + 4..at + 6].copy_from_slice(&halves[b as usize].to_le_bytes());
                        out[at + 6..at + 8].copy_from_slice(&0x3C00u16.to_le_bytes());
                    }
                    _ => {}
                }
            }
        }
        SelectObject(hdc, old_font);
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(font_bold.into());
        SelectObject(hdc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(hdc);
    }
    Some(out)
}

struct Buffers {
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
    fn new(host: &Host, format: DXGI_FORMAT, scale: u32) -> Option<Buffers> {
        let (width, height) = size(scale);
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
        Some(Buffers { format, scale, pitch, uploads, card: card?, turn: 0, fresh: false, used: false })
    }

    fn write(&mut self, rows: &[Row]) {
        let Some(picture) = render_rows(rows, self.scale, self.format, self.pitch as usize) else { return };
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
        Menu { started: values.clone(), values, at: 0, note: String::new(), changed: true, painted: None, buffers: None, failed: false }
    }

    fn open() -> bool {
        crate::SETTINGS_MENU_OPEN.load(Ordering::Relaxed)
    }

    fn rows(&self) -> Vec<Row> {
        let total = ITEMS.len() + ACTIONS.len();
        let first = self.at.saturating_sub(ROWS / 2).min(total.saturating_sub(ROWS));
        let mut out = vec![
            Row::Text("设置菜单　　　　　　　　　　F1 / Esc 关闭".to_owned()),
            Row::Text("↑↓ 选择　←→ 修改　回车 执行".to_owned()),
            Row::Text(String::new()),
        ];
        for row in first..(first + ROWS).min(total) {
            let sel = row == self.at;
            if let Some(item) = ITEMS.get(row) {
                let value = &self.values[row];
                let shown = index_of(item, value).map_or(value.to_owned(), |i| item.values[i].1.to_owned());
                let mark = if *value != self.started[row] {
                    if live(item) { " *实时".to_owned() } else { " *".to_owned() }
                } else if live(item) {
                    " 实时".to_owned()
                } else {
                    String::new()
                };
                out.push(Row::Item { sel, label: item.label.to_owned(), value: shown, mark });
            } else {
                out.push(Row::Action { sel, text: ACTIONS[row - ITEMS.len()].to_owned() });
            }
        }
        while out.len() < 3 + ROWS {
            out.push(Row::Text(String::new()));
        }
        out.push(Row::Text(String::new()));
        out.push(Row::Text(if self.note.is_empty() {
            "* = 已修改：除标注“实时”外，下次启动生效".to_owned()
        } else {
            self.note.clone()
        }));
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
            self.note = "没有设置文件：请用启动器启动游戏".to_owned();
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
                self.note = "已保存，下次启动生效".to_owned();
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "settings menu: not saved");
                self.note = "无法写入设置文件".to_owned();
                false
            }
        }
    }

    /// Starts the launcher again in a new console, a few seconds on, and ends this run.
    fn restart(&mut self) {
        use std::os::windows::process::CommandExt;
        let Some(play) = std::env::var_os("VICTORY_MENU_PLAY") else {
            self.note = "已保存，请自行重新启动启动器".to_owned();
            return;
        };
        let play = play.to_string_lossy().replace('\'', "''");
        let args = get("VICTORY_MENU_ARGS");
        let command = format!("Start-Sleep 3; & '{play}' {args}");
        const CREATE_NEW_CONSOLE: u32 = 0x10;
        match std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command])
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
        {
            Ok(_) => {
                tracing::info!(command, "settings menu: restarting");
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
                self.note = "已保存，但重启失败".to_owned();
            }
        }
    }

    /// Removes the settings file: the launcher's own settings at the next start.
    fn reset(&mut self) {
        if let Some(path) = std::env::var_os("VICTORY_MENU_FILE") {
            let _ = std::fs::remove_file(path);
        }
        self.note = "已重置，下次启动使用启动器默认".to_owned();
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
                self.note.clear();
                self.changed = true;
                continue;
            }
            if !open {
                continue;
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
                            self.restart();
                        }
                    }
                    1 => {
                        self.save();
                    }
                    2 => self.reset(),
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
        if self.buffers.as_ref().is_none_or(|b| (b.format, b.scale) != (format, scale)) {
            std::mem::forget(self.buffers.take());
            self.buffers = Buffers::new(host, format, scale);
            self.failed = self.buffers.is_none();
            self.changed = true;
        }
        if self.painted != line {
            self.painted = line.clone();
            self.changed = true;
        }
        let rows = if self.changed { Some(line.as_ref().map_or_else(|| self.rows(), |l| vec![Row::Text(l.clone())])) } else { None };
        let Some(buffers) = &mut self.buffers else { return };
        if let Some(rows) = rows {
            self.changed = false;
            buffers.write(&rows);
        }
        // The whole box in the middle, or of it the first line's part in the corner.
        let (w, h) = match &line {
            Some(l) => ((l.chars().count() as u32 * 6 + 2 * RIM) * scale, (ROW_H + 2 * RIM) * scale),
            None => size(scale),
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
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT { Format: format, Width: size(scale).0, Height: size(scale).1, Depth: 1, RowPitch: buffers.pitch },
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
    fn labels_and_values_fit_their_columns() {
        for item in ITEMS {
            assert!(item.label.chars().count() <= 24, "{}", item.label);
            for (_, shown) in item.values {
                assert!(shown.chars().count() <= 22, "{shown}");
            }
        }
        let menu = Menu { values: ITEMS.iter().map(|_| String::new()).collect(), started: Vec::new(), at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false };
        assert_eq!(menu.rows().len(), LINES as usize);
    }

    #[test]
    fn values_cycle_both_ways() {
        let mut menu = Menu { values: ITEMS.iter().map(|i| i.values[0].0.to_owned()).collect(), started: Vec::new(), at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false };
        menu.started = menu.values.clone();
        let n = ITEMS[0].values.len();
        for _ in 0..n {
            menu.change(0, 1);
        }
        assert_eq!(menu.values[0], ITEMS[0].values[0].0);
        menu.change(0, -1);
        assert_eq!(menu.values[0], ITEMS[0].values[n - 1].0);
    }

    /// Renders the real menu through GDI and writes a BMP preview to the temp
    /// folder, so the Chinese labels can be looked at without the game.
    #[test]
    fn render_preview_to_temp() {
        use std::io::Write;
        let values: Vec<String> = ITEMS.iter().map(|i| i.values[0].0.to_owned()).collect();
        let menu = Menu { started: values.clone(), values, at: 0, note: String::new(), changed: false, painted: None, buffers: None, failed: false };
        let rows = menu.rows();
        let scale = 2;
        let format = DXGI_FORMAT_B8G8R8A8_UNORM;
        let (w, h) = size(scale);
        let pitch = (w * 4).next_multiple_of(256) as usize;
        let px = render_rows(&rows, scale, format, pitch).expect("GDI render");
        let path = std::env::temp_dir().join("victory-menu-preview.bmp");
        let mut f = std::fs::File::create(&path).unwrap();
        let data_size = w * h * 4;
        let mut hdr = Vec::new();
        hdr.extend_from_slice(b"BM");
        hdr.extend_from_slice(&(14 + 40 + data_size).to_le_bytes());
        hdr.extend_from_slice(&0u16.to_le_bytes());
        hdr.extend_from_slice(&0u16.to_le_bytes());
        hdr.extend_from_slice(&54u32.to_le_bytes());
        hdr.extend_from_slice(&40u32.to_le_bytes());
        hdr.extend_from_slice(&(w as i32).to_le_bytes());
        hdr.extend_from_slice(&(h as i32).to_le_bytes());
        hdr.extend_from_slice(&1u16.to_le_bytes());
        hdr.extend_from_slice(&32u16.to_le_bytes());
        hdr.extend_from_slice(&0u32.to_le_bytes());
        hdr.extend_from_slice(&data_size.to_le_bytes());
        hdr.extend_from_slice(&0i32.to_le_bytes());
        hdr.extend_from_slice(&0i32.to_le_bytes());
        hdr.extend_from_slice(&0u32.to_le_bytes());
        hdr.extend_from_slice(&0u32.to_le_bytes());
        f.write_all(&hdr).unwrap();
        for y in (0..h).rev() {
            f.write_all(&px[y as usize * pitch..y as usize * pitch + (w * 4) as usize]).unwrap();
        }
        println!("preview: {}", path.display());
    }
}