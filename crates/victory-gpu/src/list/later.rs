//! List calls translated off the title's threads (opt-in,
//! `VICTORY_DEFER_LISTS=<workers>`).
//!
//! Without it, each list method translates its call on the title's thread
//! as it is made: address lookups, descriptor mirroring, state tracking,
//! then the host call. With it, the method copies its arguments (and what
//! they point to) into a log the list keeps, and worker threads translate
//! the log in order, as the title goes on recording. Lists translate in
//! parallel; one list's calls always run in the order the title made them,
//! one at a time, as before.
//!
//! What stays on the title's thread:
//! - methods that return something the title reads: the list type, its
//!   interfaces, upload memory (`allocate_upload`) and driver hints;
//! - `Reset`, `ResetX` and the last `Release`, which first wait until the
//!   list's logged calls have run, since they free what those calls read
//!   (upload memory, the draw state, the host lists);
//! - a submission (`crate::queue`) and a host library recording into the
//!   list (`crate::external`), which wait likewise before they read the
//!   list's host lists.
//!
//! The translation reads title memory (descriptors, buffers, textures)
//! after the call rather than at it, but still before the submission. The
//! console's GPU reads it when the list runs, later still, so a title
//! cannot rely on the earlier read.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D12::D3D12_VIEWPORT;

use super::{Barrier, CopyLocation, List, Rect, This};
use crate::draw::{IndexView, VertexView};
use crate::object::{HRESULT, S_OK};

/// Calls a worker takes at a time; the title hands its calls over in
/// batches of this many (or fewer, when something waits for them).
const BATCH: usize = 32;

/// A logged call: the method with its arguments, copied.
type Call = Box<dyn FnOnce() + Send>;

/// The calls a list logged that have not run yet.
#[derive(Default)]
pub(super) struct Log {
    inner: Mutex<Inner>,
    /// Signalled when a worker has run every batch handed over.
    idle: Condvar,
}

#[derive(Default)]
struct Inner {
    /// Logged and not handed to a worker yet.
    open: Vec<Call>,
    /// Handed over, in order.
    ready: VecDeque<Vec<Call>>,
    /// A worker runs this log's batches, or will.
    busy: bool,
}

impl Log {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Moves the open calls to the ready batches, and the log to a worker if
/// none has it.
fn hand_over(log: &Arc<Log>, inner: &mut Inner) {
    if inner.open.is_empty() {
        return;
    }
    let batch = std::mem::replace(&mut inner.open, Vec::with_capacity(BATCH));
    inner.ready.push_back(batch);
    if !inner.busy {
        inner.busy = true;
        pool().push(log.clone());
    }
}

/// The worker count `VICTORY_DEFER_LISTS` asks for: none (unset, 0, or not
/// a number) leaves every call on the title's thread.
fn workers() -> usize {
    static WORKERS: OnceLock<usize> = OnceLock::new();
    *WORKERS.get_or_init(|| {
        let n = std::env::var("VICTORY_DEFER_LISTS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0)
            .min(64);
        if n > 0 {
            tracing::info!(workers = n, "list calls translate on worker threads");
        }
        n
    })
}

/// A new list's log: `Some` if its calls are to be deferred.
pub(super) fn new_log() -> Option<Arc<Log>> {
    #[cfg(test)]
    if tests::FORCED.with(std::cell::Cell::get) {
        return Some(Arc::default());
    }
    (workers() > 0).then(Arc::default)
}

/// The workers and the logs waiting for one.
struct Pool {
    waiting: Mutex<VecDeque<Arc<Log>>>,
    wake: Condvar,
}

impl Pool {
    fn push(&self, log: Arc<Log>) {
        self.waiting.lock().unwrap_or_else(|e| e.into_inner()).push_back(log);
        self.wake.notify_one();
    }
}

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let n = workers().max(cfg!(test) as usize * 2).max(1);
        for i in 0..n {
            let spawned = std::thread::Builder::new()
                .name(format!("victory-list-{i}"))
                .spawn(work);
            if let Err(e) = spawned {
                tracing::error!(error = %e, "no list worker thread");
            }
        }
        Pool { waiting: Mutex::new(VecDeque::new()), wake: Condvar::new() }
    })
}

/// A worker: takes a log, runs its batches until none is left, then the
/// next log.
fn work() {
    let pool = pool();
    loop {
        let log = {
            let mut waiting = pool.waiting.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                match waiting.pop_front() {
                    Some(log) => break log,
                    None => waiting = pool.wake.wait(waiting).unwrap_or_else(|e| e.into_inner()),
                }
            }
        };
        loop {
            let batch = {
                let mut inner = log.lock();
                match inner.ready.pop_front() {
                    Some(batch) => batch,
                    None => {
                        inner.busy = false;
                        log.idle.notify_all();
                        break;
                    }
                }
            };
            for call in batch {
                run(call);
            }
        }
    }
}

/// Runs one call. The methods are `extern "system"`: a panic in one ends
/// the run, as it did on the title's thread.
fn run(call: Call) {
    call();
    CALLS.fetch_add(1, Relaxed);
}

/// Calls run by workers, and time title threads spent waiting for them
/// (nanoseconds), since the last report.
static CALLS: AtomicU64 = AtomicU64::new(0);
static WAITED_NS: AtomicU64 = AtomicU64::new(0);

/// Deferred calls and milliseconds waited for them since the last call
/// (the frame report); `None` with deferral off.
pub(crate) fn take_stats() -> Option<(u64, f64)> {
    (workers() > 0).then(|| (CALLS.swap(0, Relaxed), WAITED_NS.swap(0, Relaxed) as f64 / 1e6))
}

/// A list's log, if its calls are deferred.
fn log_of<'a>(this: This) -> Option<&'a Arc<Log>> {
    if this.is_null() {
        return None;
    }
    // SAFETY: the title passes one of our lists.
    unsafe { (*this.cast::<List>()).log.as_ref() }
}

/// Runs `f` now, or logs it for a worker if the list defers its calls.
/// `f` owns what it reads: the title's objects as pointers (D3D12 has them
/// live until the list has run), the rest copied.
fn later(this: This, f: impl FnOnce() + 'static) {
    let Some(log) = log_of(this) else {
        return f();
    };
    struct Sent<F>(F);
    // SAFETY: what a call captures is our list, the title's objects (used
    // from any thread, as title threads already do) and copies.
    unsafe impl<F> Send for Sent<F> {}
    impl<F: FnOnce()> Sent<F> {
        fn run(self) {
            (self.0)()
        }
    }
    let f = Sent(f);
    push(log, Box::new(move || f.run()));
}

/// Logs a call; a full batch goes to the workers.
fn push(log: &Arc<Log>, call: Call) {
    let mut inner = log.lock();
    inner.open.push(call);
    if inner.open.len() >= BATCH {
        hand_over(log, &mut inner);
    }
}

/// Waits until every call the list logged has run (nothing to wait for if
/// it does not defer). For whatever reads or frees what the calls use.
pub(crate) fn drain(this: This) {
    if let Some(log) = log_of(this) {
        settle(log);
    }
}

/// Hands the log's open calls over and waits until every call has run.
fn settle(log: &Arc<Log>) {
    let mut inner = log.lock();
    hand_over(log, &mut inner);
    if !inner.busy {
        return;
    }
    let start = std::time::Instant::now();
    while inner.busy {
        inner = log.idle.wait(inner).unwrap_or_else(|e| e.into_inner());
    }
    WAITED_NS.fetch_add(start.elapsed().as_nanos() as u64, Relaxed);
}

/// `count` items at `items` copied, or `None` for a null pointer.
fn copied<T: Copy>(items: *const T, count: u32) -> Option<Vec<T>> {
    if items.is_null() {
        return None;
    }
    // SAFETY: the title passes `count` items.
    Some(unsafe { std::slice::from_raw_parts(items, count as usize) }.to_vec())
}

/// The item at `item` copied, or `None` for a null pointer.
fn one<T: Copy>(item: *const T) -> Option<T> {
    // SAFETY: the title passes an item or null.
    unsafe { item.as_ref() }.copied()
}

/// A pointer to what [`copied`] kept, or null as the title passed.
fn at<T>(items: &Option<Vec<T>>) -> *const T {
    items.as_ref().map_or(std::ptr::null(), |v| v.as_ptr())
}

/// A pointer to what [`one`] kept, or null as the title passed.
fn at_one<T>(item: &Option<T>) -> *const T {
    item.as_ref().map_or(std::ptr::null(), std::ptr::from_ref)
}

/// Methods whose arguments are all values: logged as they are.
macro_rules! deferred {
    ($($name:ident => $($target:ident)::+ ($($arg:ident: $ty:ty),*);)*) => {$(
        extern "system" fn $name(this: This, $($arg: $ty),*) {
            later(this, move || $($target)::+(this, $($arg),*));
        }
    )*};
}

deferred! {
    draw => super::state::draw(vertices: u32, instances: u32, first: u32, first_instance: u32);
    draw_indexed => super::state::draw_indexed(indices: u32, instances: u32, first: u32, base: i32, first_instance: u32);
    dispatch => super::state::dispatch(x: u32, y: u32, z: u32);
    dispatch_mesh => super::state::dispatch_mesh(x: u32, y: u32, z: u32);
    dispatch_mesh_x => super::state::dispatch_mesh_x(x: u32, flags: u64, indirect: u64, payload: u64);
    dispatch_x => super::dispatch_x(x: u32, y: u32, z: u32, flags: u32);
    copy_buffer_region => super::copy_buffer_region(dst: This, dst_offset: u64, src: This, src_offset: u64, len: u64);
    copy_resource => super::copy_resource(dst: This, src: This);
    copy_resource_x => super::copy_resource_x(dst: This, src: This, flags: u32);
    resolve => super::resolve(dst: This, dst_sub: u32, src: This, src_sub: u32, format: u32);
    topology => super::state::topology(topology: u32);
    stencil_ref => super::state::stencil_ref(reference: u32);
    set_pipeline => super::set_pipeline(pipeline: This);
    compute_table => super::state::compute_table(slot: u32, handle: u64);
    graphics_table => super::state::graphics_table(slot: u32, handle: u64);
    compute_constant => super::state::compute_constant(slot: u32, value: u32, offset: u32);
    graphics_constant => super::state::graphics_constant(slot: u32, value: u32, offset: u32);
    compute_view => super::state::compute_view(slot: u32, address: u64);
    graphics_view => super::state::graphics_view(slot: u32, address: u64);
    execute_indirect => super::execute_indirect(signature: This, max: u32, arguments: This, offset: u64, count: This, count_offset: u64);
    copy_memory => super::copy_memory(dst: u64, src: u64, len: u64, flags: u32);
    depth_bounds => super::depth_bounds(min: f32, max: f32);
    write32 => super::write32(address: u64, value: u32, flush: u32, flags: u32);
    write64 => super::write64(address: u64, value: u64, flush: u32, flags: u32);
    wait32 => super::wait32(address: u64, comparison: u32, reference: u32, mask: u32, flags: u32);
    wait64 => super::wait64(address: u64, comparison: u32, reference: u64, flags: u32);
    resolve_queries => super::resolve_queries(heap: This, kind: u32, first: u32, count: u32, buffer: This, offset: u64);
}

extern "system" fn close(this: This) -> HRESULT {
    later(this, move || {
        super::close(this);
    });
    S_OK
}

extern "system" fn reset(this: This, allocator: This, pipeline: This) -> HRESULT {
    drain(this);
    super::reset(this, allocator, pipeline)
}

extern "system" fn reset_x(this: This, allocator: This, pipeline: This, flags: u32) -> HRESULT {
    drain(this);
    super::reset_x(this, allocator, pipeline, flags)
}

extern "system" fn release(this: This) -> u32 {
    if !this.is_null() && crate::object::refs(this) == 1 {
        drain(this);
    }
    super::list_release(this)
}

/// Returns the console's barrier "actions" (none), as the method does.
extern "system" fn barrier(this: This, count: u32, barriers: *const Barrier) -> u32 {
    let barriers = copied(barriers, count);
    later(this, move || {
        super::barrier(this, count, at(&barriers));
    });
    0
}

/// The groups' 16-byte headers are copied; the barriers they point to are
/// not read (`super::barrier_groups`).
extern "system" fn barrier_groups(this: This, count: u32, groups: *const u8) {
    let groups = copied(groups.cast::<[u32; 4]>(), count);
    later(this, move || super::barrier_groups(this, count, at(&groups).cast()));
}

extern "system" fn copy_texture_region(
    this: This,
    dst: *const CopyLocation,
    x: u32,
    y: u32,
    z: u32,
    src: *const CopyLocation,
    source_box: *const [u32; 6],
) {
    let (dst, src, source_box) = (one(dst), one(src), one(source_box));
    later(this, move || {
        super::copy_texture_region(this, at_one(&dst), x, y, z, at_one(&src), at_one(&source_box));
    });
}

#[allow(clippy::too_many_arguments)] // the method's own arguments
extern "system" fn copy_texture_region_x(
    this: This,
    dst: *const CopyLocation,
    x: u32,
    y: u32,
    z: u32,
    src: *const CopyLocation,
    source_box: *const [u32; 6],
    _flags: u32,
) {
    copy_texture_region(this, dst, x, y, z, src, source_box);
}

extern "system" fn clear_rtv(this: This, handle: u64, color: *const [f32; 4], count: u32, rects: *const Rect) {
    let (color, rects) = (one(color), copied(rects, count));
    later(this, move || super::clear_rtv(this, handle, at_one(&color), count, at(&rects)));
}

extern "system" fn clear_dsv(
    this: This,
    handle: u64,
    flags: u32,
    depth: f32,
    stencil: u8,
    count: u32,
    rects: *const Rect,
) {
    let rects = copied(rects, count);
    later(this, move || super::clear_dsv(this, handle, flags, depth, stencil, count, at(&rects)));
}

extern "system" fn viewports(this: This, count: u32, viewports: *const D3D12_VIEWPORT) {
    let viewports = copied(viewports, count);
    later(this, move || super::state::viewports(this, count, at(&viewports)));
}

extern "system" fn scissors(this: This, count: u32, rects: *const RECT) {
    let rects = copied(rects, count);
    later(this, move || super::state::scissors(this, count, at(&rects)));
}

extern "system" fn blend_factor(this: This, factor: *const [f32; 4]) {
    let factor = one(factor);
    later(this, move || super::state::blend_factor(this, at_one(&factor)));
}

extern "system" fn heaps(this: This, count: u32, heaps: *const This) {
    let heaps = copied(heaps, count);
    later(this, move || super::state::heaps(this, count, at(&heaps)));
}

extern "system" fn clear_unordered_uint(
    this: This,
    gpu: u64,
    cpu: u64,
    resource: This,
    values: *const [u32; 4],
    count: u32,
    rects: *const RECT,
) {
    let (values, rects) = (one(values), copied(rects, count));
    later(this, move || {
        super::state::clear_unordered_uint(this, gpu, cpu, resource, at_one(&values), count, at(&rects));
    });
}

extern "system" fn clear_unordered_float(
    this: This,
    gpu: u64,
    cpu: u64,
    resource: This,
    values: *const [u32; 4],
    count: u32,
    rects: *const RECT,
) {
    let (values, rects) = (one(values), copied(rects, count));
    later(this, move || {
        super::state::clear_unordered_float(this, gpu, cpu, resource, at_one(&values), count, at(&rects));
    });
}

extern "system" fn compute_constants(this: This, slot: u32, count: u32, values: *const u32, offset: u32) {
    let values = copied(values, count);
    later(this, move || super::state::compute_constants(this, slot, count, at(&values), offset));
}

extern "system" fn graphics_constants(this: This, slot: u32, count: u32, values: *const u32, offset: u32) {
    let values = copied(values, count);
    later(this, move || super::state::graphics_constants(this, slot, count, at(&values), offset));
}

extern "system" fn index_buffer(this: This, view: *const IndexView) {
    let view = one(view);
    later(this, move || super::state::index_buffer(this, at_one(&view)));
}

extern "system" fn vertex_buffers(this: This, start: u32, count: u32, views: *const VertexView) {
    let views = copied(views, count);
    later(this, move || super::state::vertex_buffers(this, start, count, at(&views)));
}

/// With one range only the first handle is read (`super::state::targets`).
extern "system" fn targets(this: This, count: u32, handles: *const u64, one_range: i32, depth: *const u64) {
    let handles = copied(handles, if one_range != 0 { count.min(1) } else { count });
    let depth = one(depth);
    later(this, move || super::state::targets(this, count, at(&handles), one_range, at_one(&depth)));
}

/// The deferring methods by their slot in older titles' lists, in place of
/// the ones in `super::METHODS` (newer lists move them as those).
pub(super) const METHODS: [(usize, *const ()); 51] = [
    (2, release as *const ()),
    (9, close as *const ()),
    (10, reset as *const ()),
    (12, draw as *const ()),
    (13, draw_indexed as *const ()),
    (14, dispatch as *const ()),
    (15, copy_buffer_region as *const ()),
    (16, copy_texture_region as *const ()),
    (17, copy_resource as *const ()),
    (19, resolve as *const ()),
    (20, topology as *const ()),
    (21, viewports as *const ()),
    (22, scissors as *const ()),
    (23, blend_factor as *const ()),
    (24, stencil_ref as *const ()),
    (25, set_pipeline as *const ()),
    (26, barrier as *const ()),
    (30, resolve_queries as *const ()),
    (32, heaps as *const ()),
    (35, compute_table as *const ()),
    (36, graphics_table as *const ()),
    (37, compute_constant as *const ()),
    (38, graphics_constant as *const ()),
    (39, compute_constants as *const ()),
    (40, graphics_constants as *const ()),
    (41, compute_view as *const ()),
    (42, graphics_view as *const ()),
    (43, compute_view as *const ()),
    (44, graphics_view as *const ()),
    (45, compute_view as *const ()),
    (46, graphics_view as *const ()),
    (47, index_buffer as *const ()),
    (48, vertex_buffers as *const ()),
    (50, targets as *const ()),
    (51, clear_dsv as *const ()),
    (52, clear_rtv as *const ()),
    (53, clear_unordered_uint as *const ()),
    (54, clear_unordered_float as *const ()),
    (59, execute_indirect as *const ()),
    (60, copy_memory as *const ()),
    (68, depth_bounds as *const ()),
    (69, write32 as *const ()),
    (70, write64 as *const ()),
    (71, wait32 as *const ()),
    (72, wait64 as *const ()),
    (78, dispatch_x as *const ()),
    (83, copy_texture_region_x as *const ()),
    (84, copy_resource_x as *const ()),
    (98, dispatch_mesh_x as *const ()),
    (127, dispatch_mesh as *const ()),
    (137, reset_x as *const ()),
];

/// The same for `super::NEWER_METHODS`, at their slot as it is.
pub(super) const NEWER_METHODS: [(usize, *const ()); 1] = [(146, barrier_groups as *const ())];

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use super::{BATCH, Log, push, settle};

    thread_local! {
        /// Lists made on this thread defer their calls, whatever
        /// `VICTORY_DEFER_LISTS` says.
        pub(crate) static FORCED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Several logs filled at once, as several title threads record: each
    /// runs its calls in the order they were logged, all of them by the
    /// time it settles, and none on the thread that logged it.
    #[test]
    fn each_log_runs_its_calls_in_order_on_workers() {
        const CALLS: usize = BATCH * 20 + 7;
        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    let log = Arc::new(Log::default());
                    let ran = Arc::new(Mutex::new(Vec::new()));
                    let me = std::thread::current().id();
                    for i in 0..CALLS {
                        let ran = ran.clone();
                        push(&log, Box::new(move || ran.lock().unwrap().push((i, std::thread::current().id()))));
                        if i % 97 == 0 {
                            std::thread::yield_now();
                        }
                    }
                    settle(&log);
                    let ran = std::mem::take(&mut *ran.lock().unwrap());
                    assert_eq!(ran.iter().map(|r| r.0).collect::<Vec<_>>(), (0..CALLS).collect::<Vec<_>>());
                    assert!(ran.iter().all(|r| r.1 != me), "a call ran on the logging thread");
                    // Settled with nothing open: returns at once, and the
                    // log takes calls again.
                    settle(&log);
                    let again = Arc::new(Mutex::new(0));
                    let a = again.clone();
                    push(&log, Box::new(move || *a.lock().unwrap() += 1));
                    settle(&log);
                    assert_eq!(*again.lock().unwrap(), 1);
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
    }
}
