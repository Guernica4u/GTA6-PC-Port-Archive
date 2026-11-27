//! Frame timing and the performance panel's text and graph. Pure functions with
//! no platform calls.

use std::collections::VecDeque;

/// Frames kept for the 1% low and the graph.
pub(crate) const HISTORY: usize = 1000;
const SECOND_US: u64 = 1_000_000;

/// Time between presented frames, newest last, in microseconds.
#[derive(Clone, Default)]
pub(crate) struct Frames(VecDeque<u32>);

impl Frames {
    pub(crate) fn push(&mut self, microseconds: u32) {
        if self.0.len() == HISTORY {
            self.0.pop_front();
        }
        self.0.push_back(microseconds);
    }

    /// The newest frames that together span a second, or all there are.
    fn last_second(&self) -> impl Iterator<Item = u32> + '_ {
        let mut spanned = 0u64;
        self.0.iter().rev().copied().take_while(move |&frame| {
            let wanted = spanned < SECOND_US;
            spanned += u64::from(frame);
            wanted
        })
    }

    /// Frames per second over the last second.
    pub(crate) fn fps(&self) -> Option<f32> {
        let (count, total) = self.last_second().fold((0u32, 0u64), |(n, t), f| (n + 1, t + u64::from(f)));
        (total > 0).then(|| (f64::from(count) * SECOND_US as f64 / total as f64) as f32)
    }

    /// Average and longest frame time over the last second, in milliseconds.
    pub(crate) fn times(&self) -> Option<(f32, f32)> {
        let (count, total, longest) = self.last_second()
            .fold((0u32, 0u64, 0u32), |(n, t, m), f| (n + 1, t + u64::from(f), m.max(f)));
        (count > 0).then(|| ((total as f64 / f64::from(count) / 1000.0) as f32, longest as f32 / 1000.0))
    }

    /// Frames per second of the slowest hundredth of the kept frames (at least
    /// one frame): the "1% low".
    pub(crate) fn low(&self) -> Option<f32> {
        let mut sorted: Vec<u32> = self.0.iter().copied().collect();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        sorted.truncate((sorted.len() / 100).max(1));
        let total: u64 = sorted.iter().map(|&f| u64::from(f)).sum();
        (total > 0).then(|| (sorted.len() as f64 * SECOND_US as f64 / total as f64) as f32)
    }

    /// The newest `count` frame times in milliseconds, oldest first.
    pub(crate) fn recent(&self, count: usize) -> impl Iterator<Item = f32> + '_ {
        self.0.iter().skip(self.0.len().saturating_sub(count)).map(|&f| f as f32 / 1000.0)
    }
}

/// What the host reports. A reading the host does not give stays `None`.
#[derive(Clone, Copy, Default)]
pub(crate) struct Host {
    /// Share of all processors in use, and by this process, as 0 to 100.
    pub(crate) cpu_system: Option<f32>,
    pub(crate) cpu_game: Option<f32>,
    /// This process's resident memory and committed memory, in bytes.
    pub(crate) game_resident: Option<u64>,
    pub(crate) game_committed: Option<u64>,
    /// System memory in use and installed.
    pub(crate) ram: Option<(u64, u64)>,
    /// Swap or page file in use and available in all.
    pub(crate) swap: Option<(u64, u64)>,
    /// This process's video memory and the budget the host gives it.
    pub(crate) video: Option<(u64, u64)>,
    pub(crate) gpu: Gpu,
}

/// The graphics card's sensors. A reading the host does not give stays `None`.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Gpu {
    /// Share of time the card was busy, as 0 to 100.
    pub(crate) load: Option<f32>,
    pub(crate) celsius: Option<f32>,
    /// Graphics clock.
    pub(crate) megahertz: Option<f32>,
    pub(crate) watts: Option<f32>,
}

/// A PCI identifier as Linux writes it in sysfs, such as `0x1a2b`.
pub(crate) fn pci_id(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim().strip_prefix("0x")?, 16).ok()
}

/// Where an entry of `WINEDLLPATH` that holds wine-nvml keeps its Windows DLL,
/// as a path through Wine's Z: drive.
pub(crate) fn wine_nvml_path(directory: &str) -> Option<String> {
    (directory.starts_with('/') && directory.trim_end_matches('/').ends_with("/nvml/wine")).then(|| {
        format!(r"Z:{}\x86_64-windows\nvml.dll", directory.trim_end_matches('/').replace('/', "\\"))
    })
}

/// A sysfs number counted in `1/unit` of the wanted one.
pub(crate) fn scaled(text: &str, unit: f64) -> Option<f32> {
    text.trim().parse::<u64>().ok().map(|value| (value as f64 / unit) as f32)
}

/// TuxBox's own work, shown beside the host's numbers.
#[derive(Clone, Copy, Default)]
pub(crate) struct Work {
    pub(crate) translating: u32,
    pub(crate) building: u32,
    pub(crate) empty_pipelines: u32,
}

/// Part of `total` as 0 to 100. Counters that did not advance give nothing.
pub(crate) fn share(part: u64, total: u64) -> Option<f32> {
    (total > 0).then(|| (part as f64 * 100.0 / total as f64).clamp(0.0, 100.0) as f32)
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1u32 << 30)
}

fn number(value: Option<f32>, digits: usize) -> String {
    value.map_or_else(|| "-".to_string(), |v| format!("{v:.digits$}"))
}

pub(crate) const LINES: usize = 9;

/// The panel's lines of text.
pub(crate) fn lines(frames: &Frames, host: &Host, work: &Work) -> [String; LINES] {
    let times = frames.times();
    let game = match (host.game_resident, host.game_committed) {
        (Some(resident), Some(committed)) => {
            format!("Game {:.1} GB  committed {:.1} GB", gib(resident), gib(committed))
        }
        _ => "Game memory -".to_string(),
    };
    let pair = |name: &str, value: Option<(u64, u64)>| match value {
        Some((used, total)) => format!("{name} {:.1} / {:.1} GB", gib(used), gib(total)),
        None => format!("{name} -"),
    };
    [
        format!("FPS {}   1% low {}", number(frames.fps(), 1), number(frames.low(), 1)),
        format!("Frame {} ms   longest {} ms", number(times.map(|t| t.0), 1), number(times.map(|t| t.1), 1)),
        format!("CPU {}%   game {}%", number(host.cpu_system, 0), number(host.cpu_game, 0)),
        format!(
            "GPU {}%  {} C  {} MHz  {} W",
            number(host.gpu.load, 0),
            number(host.gpu.celsius, 0),
            number(host.gpu.megahertz, 0),
            number(host.gpu.watts, 0)
        ),
        game,
        pair("RAM", host.ram),
        pair("Swap", host.swap),
        pair("VRAM", host.video),
        format!(
            "Shaders {} | building {} | empty {}",
            work.translating, work.building, work.empty_pipelines
        ),
    ]
}

/// Frame times at the top of the graph, in milliseconds.
pub(crate) const GRAPH_MS: f32 = 50.0;

/// The height of one graph bar: at least a pixel, and the full graph from `GRAPH_MS`.
pub(crate) fn bar(milliseconds: f32, height: u32) -> u32 {
    ((milliseconds / GRAPH_MS * height as f32) as u32).clamp(1, height.max(1))
}

#[cfg(test)]
#[path = "../tests/performance/stats.rs"]
mod tests;
