//! A few threads kept for [`crate::host::in_parallel`], started on first use.
//!
//! `std::thread::scope` started up to five threads afresh on every call, and
//! the submitting thread calls it several times a submission (snapshots,
//! read-backs, bounce copies): about 1.2% of that thread went to creating
//! threads (`ZwCreateThreadEx`, issue #163).
//!
//! A call offers its work to the kept threads and does its own share. Those
//! that take it before the caller is done help; the offer is then withdrawn
//! and the caller waits only for the threads that took it. So a call never
//! waits for a thread busy elsewhere (another call, or one nested in its own
//! work), and a call no thread helps runs as it would alone.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};

/// Threads kept besides the callers'.
pub(crate) const HELPERS: usize = 5;

/// One call's work as offered to the kept threads.
struct Job {
    /// The caller's work with its lifetime erased. A thread calls it only
    /// after taking a ticket, and the caller waits for every thread that
    /// took one before it returns, so the work outlives every call.
    work: *const (dyn Fn() + Sync + 'static),
    state: Mutex<State>,
    done: Condvar,
}

#[derive(Default)]
struct State {
    /// Threads that may still join; zero once the caller is done.
    tickets: usize,
    /// Threads that joined and are not done.
    running: usize,
    /// The first panic of a thread that joined.
    panic: Option<Box<dyn Any + Send>>,
}

// SAFETY: `work` is `Sync` and only called while the caller waits (see
// `Job::work`); the rest is `Send` and `Sync` on its own.
unsafe impl Send for Job {}
// SAFETY: as above.
unsafe impl Sync for Job {}

impl Job {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Where calls offer their work; `None` if no thread could be started.
fn pool() -> Option<&'static Sender<Arc<Job>>> {
    static POOL: OnceLock<Option<Sender<Arc<Job>>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (sender, receiver) = channel::<Arc<Job>>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut started = 0;
        for i in 0..HELPERS {
            let receiver = receiver.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("victory-parallel-{i}"))
                .spawn(move || helper(&receiver));
            match spawned {
                Ok(_) => started += 1,
                Err(e) => tracing::warn!(error = %e, "no parallel helper thread"),
            }
        }
        (started > 0).then_some(sender)
    })
    .as_ref()
}

/// A kept thread: takes offers in turn and helps with those still open.
fn helper(jobs: &Mutex<Receiver<Arc<Job>>>) {
    loop {
        let next = jobs.lock().unwrap_or_else(|e| e.into_inner()).recv();
        let Ok(job) = next else { return };
        {
            let mut s = job.lock();
            if s.tickets == 0 {
                continue;
            }
            s.tickets -= 1;
            s.running += 1;
        }
        // SAFETY: a ticket was taken while the caller waits (see `Job::work`).
        let result = catch_unwind(AssertUnwindSafe(|| unsafe { (*job.work)() }));
        let mut s = job.lock();
        s.running -= 1;
        if let Err(payload) = result {
            s.panic.get_or_insert(payload);
        }
        if s.running == 0 {
            job.done.notify_all();
        }
    }
}

/// Runs `work` on this thread and on up to `helpers` kept threads at once,
/// and returns when every run of it has returned. `work` shares itself out
/// (it takes pieces until none are left). A panic in any run is raised here.
pub(crate) fn run(helpers: usize, work: &(dyn Fn() + Sync)) {
    let helpers = helpers.min(HELPERS);
    let sender = match pool() {
        Some(sender) if helpers > 0 => sender,
        _ => return work(),
    };
    let short: *const (dyn Fn() + Sync + '_) = work;
    // SAFETY: only the lifetime changes; see `Job::work` for why the
    // longer one is never relied on.
    let work_static =
        unsafe { std::mem::transmute::<*const (dyn Fn() + Sync + '_), *const (dyn Fn() + Sync + 'static)>(short) };
    let job = Arc::new(Job {
        work: work_static,
        state: Mutex::new(State { tickets: helpers, ..State::default() }),
        done: Condvar::new(),
    });
    for _ in 0..helpers {
        if sender.send(job.clone()).is_err() {
            break;
        }
    }
    let mine = catch_unwind(AssertUnwindSafe(work));
    let theirs = {
        let mut s = job.lock();
        s.tickets = 0;
        while s.running > 0 {
            s = job.done.wait(s).unwrap_or_else(|e| e.into_inner());
        }
        s.panic.take()
    };
    if let Err(payload) = mine {
        resume_unwind(payload);
    }
    if let Some(payload) = theirs {
        resume_unwind(payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// Shares `0..n` out in pieces of `piece`, as `in_parallel` does.
    fn share(n: usize, piece: usize, helpers: usize, f: impl Fn(usize) + Sync) {
        let next = AtomicUsize::new(0);
        run(helpers, &|| {
            loop {
                let from = next.fetch_add(piece, Ordering::Relaxed);
                if from >= n {
                    break;
                }
                (from..(from + piece).min(n)).for_each(&f);
            }
        });
    }

    #[test]
    fn every_item_is_done_once() {
        let seen: Vec<AtomicUsize> = (0..10_000).map(|_| AtomicUsize::new(0)).collect();
        share(seen.len(), 7, HELPERS, |i| {
            seen[i].fetch_add(1, Ordering::Relaxed);
        });
        assert!(seen.iter().all(|s| s.load(Ordering::Relaxed) == 1));
    }

    #[test]
    fn the_threads_are_kept() {
        let ids = Mutex::new(std::collections::HashSet::new());
        for _ in 0..50 {
            share(600, 1, HELPERS, |_| {
                if std::thread::current().name().is_some_and(|n| n.starts_with("victory-parallel-")) {
                    ids.lock().unwrap().insert(std::thread::current().id());
                }
                std::hint::black_box((0..200).sum::<u64>());
            });
        }
        let ids = ids.into_inner().unwrap();
        assert!(ids.len() <= HELPERS, "{} helper threads", ids.len());
    }

    #[test]
    fn a_call_inside_a_call_returns() {
        let total = AtomicUsize::new(0);
        share(64, 1, HELPERS, |_| {
            share(64, 1, HELPERS, |_| {
                total.fetch_add(1, Ordering::Relaxed);
            });
        });
        assert_eq!(total.load(Ordering::Relaxed), 64 * 64);
    }

    #[test]
    fn calls_from_many_threads_at_once() {
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..20 {
                        let sum = AtomicUsize::new(0);
                        share(1000, 3, HELPERS, |i| {
                            sum.fetch_add(i, Ordering::Relaxed);
                        });
                        assert_eq!(sum.load(Ordering::Relaxed), 999 * 1000 / 2);
                    }
                });
            }
        });
    }

    #[test]
    fn a_panic_in_a_kept_thread_reaches_the_caller() {
        let joined = AtomicBool::new(false);
        let result = catch_unwind(AssertUnwindSafe(|| {
            run(HELPERS, &|| {
                if std::thread::current().name().is_some_and(|n| n.starts_with("victory-parallel-")) {
                    joined.store(true, Ordering::Release);
                    panic!("a helper's piece panicked");
                }
                // The caller stays until a kept thread has joined.
                let started = Instant::now();
                while !joined.load(Ordering::Acquire) && started.elapsed() < Duration::from_secs(5) {
                    std::thread::yield_now();
                }
            });
        }));
        assert!(joined.load(Ordering::Acquire), "no kept thread joined within 5 s");
        assert!(result.is_err());
        // The pool still works afterwards.
        every_item_is_done_once();
    }

    #[test]
    fn no_helpers_runs_on_the_caller() {
        let me = std::thread::current().id();
        run(0, &|| assert_eq!(std::thread::current().id(), me));
    }
}
