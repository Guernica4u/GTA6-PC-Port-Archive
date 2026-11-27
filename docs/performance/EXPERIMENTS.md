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
