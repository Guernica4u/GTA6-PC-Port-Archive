//! A bounded capture in every build. The title's frame token, host list pointer,
//! recording ID and CPU clock let the command, queue and Present rows be joined.
//! GPU timestamps are in ticks. `execute` reports the queue's frequency and a
//! pair of GPU and CPU performance-counter timestamps for calibration.

use std::io::{BufWriter, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::time::Duration;

use super::{now_us, thread_id};

struct Capture {
    start_us: u64,
    end_us: u64,
    sender: SyncSender<String>,
}

static DROPPED: AtomicU64 = AtomicU64::new(0);

fn capture() -> Option<&'static Capture> {
    static CAPTURE: OnceLock<Option<Capture>> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let path = std::env::var_os("VICTORY_RENDER_TRACE").filter(|p| !p.is_empty() && p != "0")?;
        let seconds = |key, default| std::env::var(key).ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(default);
        let start_us = seconds("VICTORY_RENDER_TRACE_AT", 60).saturating_mul(1_000_000);
        let end_us = start_us.saturating_add(seconds("VICTORY_RENDER_TRACE_SECONDS", 3).clamp(1, 30) * 1_000_000);
        let file = match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(?path, %error, "could not open VICTORY_RENDER_TRACE");
                return None;
            }
        };
        let (sender, receiver) = sync_channel::<String>(1024);
        let worker = std::thread::Builder::new().name("victory-render-trace".into()).spawn(move || -> std::io::Result<()> {
            let mut out = BufWriter::with_capacity(256 * 1024, file);
            writeln!(out, "# VIctory render trace v1, tab-separated event and key=value fields")?;
            loop {
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(batch) => out.write_all(batch.as_bytes())?,
                    Err(RecvTimeoutError::Timeout) => {
                        let dropped = DROPPED.swap(0, Relaxed);
                        if dropped != 0 {
                            writeln!(out, "capture_loss\tbatches={dropped}")?;
                        }
                        out.flush()?;
                    }
                    Err(RecvTimeoutError::Disconnected) => return out.flush(),
                }
            }
        });
        match worker {
            Ok(_) => {
                tracing::info!(?path, start_us, end_us, "VIctory render trace enabled");
                Some(Capture { start_us, end_us, sender })
            }
            Err(error) => {
                tracing::warn!(%error, "could not start the render trace writer");
                None
            }
        }
    }).as_ref()
}

pub(crate) fn active() -> bool {
    capture().is_some_and(|c| (c.start_us..c.end_us).contains(&now_us()))
}

/// Used also for GPU results that arrive after the capture window.
pub(super) fn write(batch: String) {
    if let Some(c) = capture() {
        match c.sender.try_send(batch) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => { DROPPED.fetch_add(1, Relaxed); }
            Err(TrySendError::Disconnected(_)) => {
                static SAID: std::sync::Once = std::sync::Once::new();
                SAID.call_once(|| tracing::warn!("the render trace writer stopped"));
            }
        }
    }
}

pub(crate) fn event(kind: &str, fields: impl std::fmt::Display) {
    write(format!("{kind}\tcpu_us={}\tthread={}\tframe={}\t{fields}\n", now_us(), thread_id(), crate::frame::current()));
}

/// A command's actual host arguments. The pipeline's readable name is held
/// separately in the GPU timing module.
#[derive(Clone, Copy)]
pub(crate) struct Command(pub &'static str, pub [i64; 5]);
