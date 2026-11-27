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

## perf/submit-thread: R8 sweep without drain, R3 joined card-step copies (UNMEASURED IN-GAME), R3 gather and R7 not done

Status: implemented, `cargo test -p victory-gpu` passes except the 2 known indirect failures. **Not run in the game.**

R8 (`host.rs`, submit's sweep): the forced `settle(WHY_SYNC)` every 1.5 s behind flights is gone. The sweep only drops what no list or slot names (`strong_count == 1`, COM refcount 1) and does it through `retire()`, which holds objects until every submission started so far has finished (submission numbers are the fence values; `bury_lag` adds 16/64 on top). A flight's lists keep their objects counted until landed. `VICTORY_SWEEP_SETTLE=1` restores the drain (also kept when the up-check is on). Measure: the 10-30 ms spike every 1.5 s in frame times, and `a synchronous submission or present` in the late line. Risk: a missed holder is a freed-memory fault (device loss); soak with `SLOT_CHECK`.

R3 (`card_steps.rs::record`): not a gather kernel. Reading the code shows the report's premise is too high: a fill that goes to a descriptor slot (`To::Slot`, stage 1) records no command at all; reads are deduplicated per place (a few hundred, "2,800 fills of a few hundred places"), only copies and stage-2 buffer fills write. With `VICTORY_FILL_TITLE` on by default the small ring fills are mostly no longer taken here. The 4.6-8.6 ms "planning" is CPU planning (`does_of`, `plan`, `holder`, `mapped` page-table probes, 5.4k `constant_slot` writes), which a kernel would not touch. A real gather also needs a new PSO, a mapped upload table per `Own`, and a per-source/per-destination state scheme (NON_PIXEL_SHADER_RESOURCE/UAV instead of COPY_SOURCE/DEST, with the "both" case), and the scatter kernel in `send.rs` is one destination UAV per dispatch, so it is not reusable as is. Not done until a capture shows the command count. Done instead: runs touching in both buffers (reads may cross equal under-256-byte padding) go in one `CopyBufferRegion`; `VICTORY_JOIN_STEPS=0` restores one each. Measure with `--timing detailed` on the card-steps list.

R7 not done. Present cannot simply move: the next `present_planes` reads `GetCurrentBackBufferIndex` and records into that buffer, so it must come after the previous `Present` call, which makes the move a hand-off with a wait; Streamline's `present_begins/called/returned/ends` and the tags belong to one thread per frame (frame-gen semantics); and the resize path settles and calls `ResizeBuffers` under the presenter lock. What would be left to overlap is ~1.5 ms/frame (`Present`) plus `panel/menu/meter.frame`, which are bookkeeping (a once-a-second log). Not worth the ordering risk without an in-game Streamline test.

## perf/read-wait: unnamed "title memory read" settles (UNMEASURED, NOT COMPILED)

Status: written and read through only; the user compiles. Log play-20261127-101425-23388.log (VICTORY_CARD_STEPS=2, relay on): 0.5 settles/frame for WHY_READ, ~30 ms each, while the three named counters (copy, fill, texture) are 0. So the wait comes from a `Some(WHY_READ)` that is not counted: `place::read_by` (host.rs, end of `blocked_by`) or `card_steps::copied_read` (`late_blocked`). `asked_sizes` (host.rs send_late) is off unless `VICTORY_BVH_SIZES_WAIT=1`. Both waits end in `relay_wait` until `line.written` no longer touches the read, and `written` includes the title's 174 MB heap while a skinning flight is in the air, so the wait drains those flights (tens of ms).

Change 1 (counters): the frame line now names two more sources inside "title memory read": `a step that reads where a copy at its place writes` (LATE_STATS 18) and `a refresh or host read of where a copy done on the card writes` (19). Read the next log to see which dominates.

Change 2 (fix for the first): `place::read_by` asked whether ANY step of the submission reads the destination of a copy at its place whose flights are in the air, wherever the step stands. Submission runs steps in list order (`run_deferred`), and a copy at its place is not put into title memory when flights may write it (`Placed::prepare(free=false)`). A step that stands before the copy reads the old bytes with or without the wait, so waiting for it changes nothing. `blocked_by` now checks each step against the destinations of the copies at their place ahead of it only (`place::reads`). Invariants kept: a reader behind the copy still waits; the readers' own source checks against `written` are unchanged.

Not changed: `copied_read`. A refresh of another host copy over a card-copy destination really needs the landed bytes (the draw reads that host copy), so the wait is correct until the copy also writes that other host copy or the refresh is given a card-side order; that needs the landing to be ordered on the queue and is a larger change. If the next log shows counter 19 dominating, that is the next work item.
## perf/tight-writes: learned footprint for root UAV declarations (UNCOMPILED, UNMEASURED; default OFF)

`VICTORY_TIGHT_WRITES=1` enables `tight.rs`. Root UAVs (`Draw::root_uav`) have no size; they declared the rest of the
title's buffer as shader-written. Now, per (pipeline, root parameter), the footprint relative to the root address is
learned from what lands (hook in `copystat_footprint::gpu_wrote`, which `land_card`/`land_back` already call). Learn:
full declaration, noted per copy for 400 ms, landings inside widen the footprint. After 8 landings and 32
declarations: tight, footprint +-256 KiB in 64 KiB blocks. Tight keys declare the footprint only, except every 16th
declaration (`VICTORY_TIGHT_VERIFY`) which is full and noted; a landing outside the footprint in a noted full
declaration makes the key wide for good (warn log). Keys that never saw a landing never tighten.
Not done: table UAV views in `readable` (sized, no pipeline identity there), persistence across runs, static shader
analysis. The view and `refresh` stay wide; only `gpu_writes` is narrowed.
Why default OFF: a write outside the footprint between two verifications is not declared, so it stays on the card
until the next declared write of its 64 KiB or verification; that violates "must reach title memory before the title
is told" for that frame. An exact fallback needs a card-side whole-range compare declared as compare-and-land (host.rs
card steps, not touched here) or a shader-side bounds flag. Attribution is a superset (any noted declaration of the
copy overlapping a landing), so errors widen, never narrow. Measure: log of `footprint of 64 KiB blocks`, and
shadow/flight lines with the switch on vs off. Code was not compiled or tested (instruction); unit tests in tight.rs.

## perf/prewarm-pipelines: earlier runs' pipelines made from process start, not at the title's creation (NOT COMPILED, UNMEASURED)

Status: written by reading only; **not compiled, not run**. Targets root cause 2 of `ROOT_CAUSES.md` (about 1,360 pipelines compiled after world entry) and the idle loading screen (~30 s of static images in which nothing was prepared). Based on `perf/radical-refactor`; builds on the pipeline-order entry above. victory-shader untouched (translator hash unchanged, no cold start).

What changed
- `known::prewarm()` (called from `apps/victory/src/run.rs` right after `use_shader_cache`, only with `--execute`, so not by `victory warm`/`translate`): a thread reads `pipeline-order.txt` + `early-pipelines.txt` (`library::plan()`, first-use order), looks each key up in `known-pipelines.bin`/`known-shaders.bin`, rebuilds the `Pending` (same key, checked) and queues it as EARLY on the existing workers (translation, library load or compile, `note_time`). It runs before the title image is even loaded, so the menu and the loading screen are the time used. First run (no order/early file or no book): logs "nothing recorded" and does nothing.
- `make::adopt` (one line in `pipeline.rs::hand_out`): the title's CreatePipelineState gets the prewarmed `Arc<Pending>` for its key (made, or in the queue; a draw promotes it to REQUESTED as before). This matters for the static-sampler pipelines (54%, never in the library): they were recompiled every run, and a made `HostPipeline` is now kept and handed out instead of thrown away. If the title creates one first, its own is used and the prewarm skips the key.
- `LOADING` phase from the prewarm's start: workers other than worker 0 at NORMAL priority (usual: LOWEST), known-slow cap raised from workers/4 to workers-1, and the early and slow heaps are drained together in first-use order (slow ones were behind all others). The loader counts as one waiting pipeline so "all made" does not fire between its entries.
- Loading over (priority back to usual, one log line): a world's frame (300 or more draws and dispatches, twice in a row; menu and loading screens draw 1 to 11) or everything queued made, or nothing to make. Constants only, no env switches added (existing ones still apply: `VICTORY_PIPELINE_ORDER=0`, `VICTORY_PIPELINE_SLOW=0`, `VICTORY_PIPELINE_WORKERS`).
- `victory warm --used` now makes the pipelines in first-use order too (reads `pipeline-order.txt`).

What to look for in the log (release)
- `pipeline prewarm: pipelines of earlier runs queued, first used first` : planned / queued / not_in_the_books / unreadable / key_changed (must be 0: the rebuilt key differs from the recorded one) / created_by_the_title_first / ms.
- `pipeline prewarm: loading over ...` : why ("the title draws a world", "everything queued is made", "nothing to make ahead"), seconds, made, found_by_the_title (hand-outs that found a prewarmed one), asked_before_made (draws that asked before it was made: the remaining stall), waiting.
- At world entry: `waiting`/`mk` near 0, the existing `host pipelines made` lines (count made after entry vs the 1,360 baseline), frames over 200 ms after entry, `a pass over the screen was waited for`.
- How to test: run twice (the first only records; needs `VICTORY_PIPELINE_BOOK=1`, set by `play.rs`); compare the second against run 095317 (250-340 ms frames for ~2.5 min). A/B a changed translator (cold driver cache) to see how much of the compile fits in the loading screen.

Risks
- Normal-priority workers compete with the title's loading threads (unmeasured); a worker keeps its priority until its current pipeline ends (up to 58 s for the worst).
- The host D3D12 device is created on the prewarm thread at process start (after Streamline init, before the title's device creation); the title's `D3D12XboxCreateDevice` finds it. If something in the title's device setup must precede `host()`, this is where it shows (not found by reading).
- Two title pipelines with one key now share one `Pending` (and `host` Arc); builder pipelines may be listed twice in `BUILDER.pipelines`. Prewarmed pipelines the title never creates stay alive (bounded by the recorded set, about 5,500).
- Compiler crash/hang now costs the process, not a child; but these are the same pipelines the title queued at creation before, so no new class of crash.
- The world threshold (300 draws) is a guess from report 05's numbers; if a menu draws more, loading ends early and the prewarm continues at the usual low priority.
