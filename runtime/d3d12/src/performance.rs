//! `TUXBOX_OVERLAY` and `TUXBOX_FRAME_LOG`: frame timing and host readings
//! gathered as the title presents. Off unless one of them is set.

mod gpu;
mod host;
pub(crate) mod stats;

use stats::{Frames, Host};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Direct3D12::ID3D12Device;

/// How often the host is asked for new readings.
const SAMPLE: Duration = Duration::from_millis(500);

/// Whether `TUXBOX_OVERLAY` asks for the performance panel.
pub(crate) fn panel_wanted() -> bool {
    std::env::var("TUXBOX_OVERLAY").is_ok_and(|v| matches!(v.trim(), "1" | "on" | "always"))
}

/// The latest host readings and how many there have been.
type Latest = Mutex<(u64, Host)>;

pub(crate) struct Monitor {
    pub(crate) frames: Frames,
    pub(crate) host: Host,
    /// Rises with each host sample, so the panel redraws only when one arrived.
    /// It starts at one, so the panel is drawn on the first frame.
    pub(crate) sample: u64,
    latest: Arc<Latest>,
    started: Instant,
    previous: Option<Instant>,
    flushed: Instant,
    log: Option<BufWriter<File>>,
}

impl Monitor {
    /// A monitor when the panel or the frame log is asked for.
    pub(crate) fn from_env(device: &ID3D12Device) -> Option<Self> {
        let log = std::env::var_os("TUXBOX_FRAME_LOG").and_then(|path| {
            let opened = File::create(&path).and_then(|file| {
                let mut log = BufWriter::new(file);
                writeln!(log, "elapsed_ms,frametime_ms")?;
                Ok(log)
            });
            if let Err(error) = &opened {
                crate::diagnostics::log(&format!("frame log not written: {error}"));
            }
            opened.ok()
        });
        if log.is_none() && !panel_wanted() {
            return None;
        }
        let latest = Arc::new(Latest::default());
        if panel_wanted() {
            // SAFETY: the LUID is read from the live device.
            let adapter = unsafe { device.GetAdapterLuid() };
            // Sensor calls can take milliseconds, so they stay off the title's
            // presentation thread. The thread ends once the monitor is gone.
            let shared = Arc::downgrade(&latest);
            let spawned = std::thread::Builder::new().name("tuxbox-performance".into()).spawn(move || {
                let mut sampler = host::Sampler::new(adapter);
                while let Some(latest) = shared.upgrade() {
                    let host = sampler.read();
                    if let Ok(mut latest) = latest.lock() {
                        *latest = (latest.0 + 1, host);
                    }
                    drop(latest);
                    std::thread::sleep(SAMPLE);
                }
            });
            if let Err(error) = spawned {
                crate::diagnostics::log(&format!("host readings unavailable: {error}"));
            }
        }
        let now = Instant::now();
        Some(Self {
            frames: Frames::default(),
            host: Host::default(),
            sample: 1,
            latest,
            started: now,
            previous: None,
            flushed: now,
            log,
        })
    }

    /// Note that a frame is being presented. Never waits for the sampler.
    pub(crate) fn frame(&mut self) {
        let now = Instant::now();
        if let Some(previous) = self.previous.replace(now) {
            let microseconds = u32::try_from((now - previous).as_micros()).unwrap_or(u32::MAX);
            self.frames.push(microseconds);
            if let Some(log) = &mut self.log {
                let _ = writeln!(
                    log,
                    "{},{:.3}",
                    (now - self.started).as_millis(),
                    f64::from(microseconds) / 1000.0
                );
            }
        }
        if let Ok(latest) = self.latest.try_lock()
            && latest.0 + 1 > self.sample
        {
            (self.sample, self.host) = (latest.0 + 1, latest.1);
        }
        // The title can end without releasing the device.
        if now - self.flushed >= SAMPLE {
            self.flushed = now;
            if let Some(log) = &mut self.log {
                let _ = log.flush();
            }
        }
    }
}
