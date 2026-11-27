# Performance experiments

## perf/pipeline-order: early queue by first use, loading boost, make-time log (UNMEASURED IN-GAME)

Status: implemented, builds, unit tests pass. **Not run in the game. No frame-time or load-time result exists yet.** Opportunity #2 of `AGENT_REPORTS/04-shader-pipeline.md` plus item 4 (instrumentation). Touches `pipeline/{make,library}.rs` only; victory-shader and the static-sampler library bypass are unchanged (no translator hash change, no cold start).

What changed
- New `pipeline-order.txt` (cache folder): `key ms` per line, first-used first; line number is the rank. Written with the library/early-list save. `early-pipelines.txt` and `slow-pipelines.txt` keep their formats (the early file is now written in first-use order, still readable as a set by older builds). A missing file means no order (creation order, as before).
- EARLY and slow queues are binary heaps by recorded rank; unranked pipelines go last in creation order. `VICTORY_PIPELINE_ORDER=0` restores FIFO.
- Boost: when 32 or more pipelines wait, workers (not the screen workers) go from LOWEST to BELOW_NORMAL (`VICTORY_PIPELINE_BOOST=normal` for NORMAL, `0` to disable) and the slow cap (workers/4) rises to workers-1. Cleared when the queue drains ("all made").
- Release log: `host pipelines {loaded from the library | made | made, static samplers (never in the library)}` with count, p50/p90/p99/max ms, total_s. Emitted at "all made" and at most once a minute with saves. The static-sampler line answers whether the driver cache makes those compiles quick.

How to measure (to do)
1. Run once to write `pipeline-order.txt` (the first run only records; ordering applies from the second).
2. Short-loading start, compare frames over 200 ms after world entry and `mk` at world entry against run 095317 (250-340 ms frames for ~2.5 min).
3. Read the new `made, static samplers` p50/p99 lines. p50 in the low ms means the driver cache hits.
4. A/B `VICTORY_PIPELINE_BOOST=0` and `VICTORY_PIPELINE_ORDER=0`.

Risks: BELOW_NORMAL workers may take CPU from the title's loading threads (unmeasured); a worker keeps its priority until its current pipeline ends; with the cap lifted all but one worker can sit in 10-58 s compiles.
# Experiments log

Status words: MEASURED (in-game numbers), UNMEASURED (built and unit-tested, not run in-game), NOT DONE (reason given).

## perf/descriptor-sync: report 03 item C (gate and coalesce heap rounds, skip ring pages)

### Step 0: counters (UNMEASURED in-game)
Added, logged with the existing "descriptor records ..." frame line (`descriptor.rs` `sync_line`):
- "the heaps' records, rounds per frame": rounds run, rounds with no changed page, kernel write-watch asks
  (`Tracked::take`, one per heap per round), ms in asks that found nothing vs found pages (E4, as far as
  `descriptor.rs` can see: the kernel call and the journal work inside `watch::writes` are not separable
  without editing `watch.rs`, which this branch does not touch), ring pages (more than 90% of the changed
  records on the page were "of draws' tables"), MB read on them, records changed on them (E5).
- "view creation per frame": buffer views and raw twins created in `buffer_srv` with their time in
  `CreateShaderResourceView` (E2; `VICTORY_DESCRIPTOR_TIMING=0` counts without timing; clock read about 25 ns
  twice per view, so about 1 ms thread time a frame at 17k twins; unmeasured).
- E1 (switch gating the heap buffer pass) and E3 (picked bitmap) need `draw.rs` / shader emit: not done here.
What to read: ring MB vs total MB read ("MB read" in the records line) says how much of the 16.5-17.8 MB a frame
step 1 could skip; "no changed page" rounds say how many of the 17 rounds a frame are pure overhead.

### Step 1, skip ring pages: NOT DONE (not behaviour-preserving without the picked bitmap)
A new record on a ring page that no draw has read yet is indistinguishable from one a shader will pick by
computed index; `copied_tabled` is only stamped after a draw reads the slot, i.e. after the round that would
have mirrored it. Skipping the page's read/compare leaves a bindless-picked slot with a stale or null host
descriptor, which violates R4 (a slot must not show a texture that is not uploaded, or freed memory;
ARCHITECTURE_AUDIT.md 2.1). The report itself lists this as the medium-risk step and makes it depend on
opportunity A (GPU-written picked bitmap) or a periodic sweep, and a sweep changes when picked slots update,
which is not provably identical. Needs the in-game ring-page counter above first.

### Step 2, rounds only for submissions with pick-by-index draws: NOT DONE (needs draw.rs)
The signal ("a list recorded a draw whose shader has SLOT_STRIDES / SLOT_TABLE_STRIDES, or uses `lent_heap`")
exists only in `draw.rs` (`heap_buffers`, `heap_strides`, around draw.rs:2046-2080, and 1878). Setting a flag
there is out of this branch's scope. Without it, gating on anything else (frame, page counts) would skip
rounds that other readers need: draws read their tables directly, but compute and `lent_heap` dispatches
read heap slots by index, and a descriptor the title copies after its last API call is mirrored only by a
round. Suggested follow-up: a `crate::heap_marks::note_pick()` thread-local set from `heap_buffers`,
`heap_strides` and the lent-heap path, read by `host.rs submit` to choose `begin_sync_heaps_after(mark)` or
reuse the previous round (dirty ranges accumulate in `Tracked::seen`, so one later round still sees all
writes). Keep the invariant that a round is never skipped for a submission whose lists have the flag.

### Step 3, coalesce write-watch asks: NOT DONE (would corrupt shared journal state)
The existing coalescing tool, `watch::ask_ahead`, writes the global journal's `ahead` set and
`ask_ahead_done` clears it. That state belongs to the submitting thread's refresh groups; the heap-sync
thread runs concurrently and would clobber it (and `journal().kept` makes it a no-op while a submission's
`ask_once` stands). A coalesced ask across both heaps needs a new entry in `watch.rs` (and a flat per-page
`seen` instead of the `BTreeMap` mutex), which is out of scope. Expected gain is also small: a round is
one kernel call per heap (usually one or two shader-visible heaps), not one per page.

### Invariants kept
No behaviour changed: descriptor slots, dynamic-slot release on GPU completion and upload-before-show are
untouched; only atomics and clock reads were added.

## Branch perf/memory-footprint (report 02: measurement, R4, R3)

All three items are UNMEASURED in-game. Build: `cargo build --release -p victory` ok; `cargo test -p victory-gpu`
415 pass, 2 fail (`indirect::tests::host_list_budget_starts_empty_at_each_submission`, `warp_batch_charges_every_execution`),
both fail identically on the base branch.

### 1. Footprint counters (measurement only) - `copystat_footprint.rs`
Per host copy, distinct 64 KiB blocks (since the copy was made): written by shaders (bytes that landed in title
memory: `land_card` / `land_back`, merged > 0), changed by the title's CPU after shaders had written them
(`Shadow::take` runs, `take_landed` non-echo, `copy`), and written by the title while pending
(`merge_under_writes` runs). Logged every ~10 s from the frame-line block in `host.rs` (not diagnostics-gated):
one total line and the top 8 copies. `VICTORY_FOOTPRINT=0` turns it off. Cost: one global mutex per event
(hundreds per frame), 1 bit per block.
Read it for: shader-written % of the 174 MB heap's 2,784 blocks (report 02 R1 decision: under ~20 % means a learned
footprint pays), and `pending title` vs `also shaders'` (pending-and-never-shader blocks are the false sharing behind
the 9.6 ms of merge-under-writes).
Caveats: "then title" is an upper bound where a refresh copies whole pages (`copy()`), and for copies in PC memory a
landing's own write shows as a title write (the card path filters those with the echo compare, the PC path does not).

### 2. R4, batched write-watch ask: mostly already there; one small change
Per-submission batching exists and is on by default (`VICTORY_ASK_ONCE`: `watch::ask_once` -> `ask_ahead` merges
ranges within 8 MiB, `ask_rest` takes what is left). Report 02's many asks therefore come from asks that miss the
standing journal (after `ask_once_end`, `merge_under_writes` -> `writes`, flights' late merges), not from the
submission's own refresh groups. Done: `ask_rest` sorts ranges by start, widest first, so a range contained in one
just asked is answered by it (before, refresh order could ask the small range and then the big one holding it).
Same pages found, same journal, never more kernel calls than before. Expected gain: small, unmeasured.
Not done: one ask per region per FRAME. A title write between two submissions of a frame would then be seen a frame
late by the later submission's refresh (stale bytes on the card), which breaks "title writes reach the copy before
the submission that reads them". A real gain needs the miss counts first (diagnostics build: `copystat` watches per
frame, split by caller).

### 3. R3, per-copy write epoch so `defer_refresh` is one compare: NOT DONE
`defer_refresh` only records a `Step::Refresh` (which also keeps the copy alive until the list is reset); the work
(`nothing_new`, `update`) runs at submission. Skipping the step at record time on an epoch compare would read the
copy's state before the title's later writes in the same frame: a write between recording and submission would be
missed (audit invariant R2). Doing the compare at submission instead needs a per-copy epoch bumped on every journal
`note()` for pages of that copy, i.e. a page -> copy map on the fault/ask path (`watch.rs on_write`, `Journal::note`);
a global epoch is useless since the title writes every frame. That is a new structure on the hottest path and needs
the step 1 counters and a diagnostics run first.
