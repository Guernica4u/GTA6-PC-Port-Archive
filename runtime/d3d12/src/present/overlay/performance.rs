//! The performance panel: frame rate, processor and memory use, TuxBox's own
//! work and a graph of recent frame times.

use super::{ACCENT, Drawing, MUTED, Placement, Surface, TEXT, TRACK, WARNING, rasterize};
use super::layout::Corner;
use crate::performance::{Monitor, panel_wanted, stats};
use crate::progress;
use windows::Win32::Graphics::Direct3D12::ID3D12Device;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;

const WIDTH: u32 = 340;
const HEIGHT: u32 = 236;
/// Bars in the graph, each `BAR` pixels wide at 1080p.
const BARS: u32 = 105;
const BAR: u32 = 3;
const GRAPH_HEIGHT: u32 = 48;

pub(in super::super) struct PerformancePanel {
    wanted: bool,
    surface: Surface,
    drawn: u64,
}

impl PerformancePanel {
    /// `TUXBOX_OVERLAY_POS` sets the corner, top left by default.
    pub(in super::super) fn new(format: DXGI_FORMAT, width: u32, height: u32) -> Self {
        let corner = match std::env::var("TUXBOX_OVERLAY_POS") {
            Ok(value) => Corner::parse(Some(&value)),
            Err(_) => Corner::TopLeft,
        };
        Self {
            wanted: panel_wanted(),
            surface: Surface::new(format, (width, height), (WIDTH, HEIGHT), corner),
            drawn: 0,
        }
    }

    /// Redraws when the monitor has new host readings, twice a second, so the
    /// numbers stay readable and drawing costs little.
    pub(in super::super) fn prepare(&mut self, device: &ID3D12Device, index: usize, monitor: &Monitor) -> Option<Placement> {
        if !self.wanted || self.surface.origin.is_none() {
            return None;
        }
        if self.drawn != monitor.sample {
            let drawing = draw(self.surface.size, self.surface.scale, monitor);
            // SAFETY: GDI objects are created, used and released inside the call.
            self.surface.set(unsafe { rasterize(self.surface.size, self.surface.scale, &drawing) }?);
            self.drawn = monitor.sample;
        }
        self.surface.place(device, index)
    }
}

fn draw(size: (u32, u32), scale: u32, monitor: &Monitor) -> Drawing {
    let mut drawing = Drawing::panel(size, scale);
    let snapshot = progress::snapshot();
    let work = stats::Work {
        translating: snapshot.in_flight,
        building: snapshot.building,
        empty_pipelines: snapshot.stand_ins,
    };
    let lines = stats::lines(&monitor.frames, &monitor.host, &work);
    let colours = [ACCENT, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, MUTED];
    drawing.lines(scale, lines.into_iter().zip(colours));
    // One bar per frame, newest at the right, with lines at 60 and 30 per second.
    let (pad, height) = (12 * scale, GRAPH_HEIGHT * scale);
    let bottom = size.1 - 10 * scale;
    drawing.fills.push((pad, bottom - height, BARS * BAR * scale, height, TRACK));
    let recent: Vec<f32> = monitor.frames.recent(BARS as usize).collect();
    let first = BARS - recent.len() as u32;
    for (index, milliseconds) in recent.into_iter().enumerate() {
        let bar = stats::bar(milliseconds, height);
        let colour = if milliseconds > 33.4 { WARNING } else { ACCENT };
        drawing.fills.push((pad + (first + index as u32) * BAR * scale, bottom - bar, (BAR - 1) * scale, bar, colour));
    }
    for mark in [16.7, 33.3] {
        drawing.fills.push((pad, bottom - stats::bar(mark, height), BARS * BAR * scale, scale, MUTED));
    }
    drawing
}
