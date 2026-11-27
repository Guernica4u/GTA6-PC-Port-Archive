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

## perf/submit-thread: R8 sweep without drain, R3 joined card-step copies (UNMEASURED IN-GAME), R3 gather and R7 not done

Status: implemented, `cargo test -p victory-gpu` passes except the 2 known indirect failures. **Not run in the game.**

R8 (`host.rs`, submit's sweep): the forced `settle(WHY_SYNC)` every 1.5 s behind flights is gone. The sweep only drops what no list or slot names (`strong_count == 1`, COM refcount 1) and does it through `retire()`, which holds objects until every submission started so far has finished (submission numbers are the fence values; `bury_lag` adds 16/64 on top). A flight's lists keep their objects counted until landed. `VICTORY_SWEEP_SETTLE=1` restores the drain (also kept when the up-check is on). Measure: the 10-30 ms spike every 1.5 s in frame times, and `a synchronous submission or present` in the late line. Risk: a missed holder is a freed-memory fault (device loss); soak with `SLOT_CHECK`.

R3 (`card_steps.rs::record`): not a gather kernel. Reading the code shows the report's premise is too high: a fill that goes to a descriptor slot (`To::Slot`, stage 1) records no command at all; reads are deduplicated per place (a few hundred, "2,800 fills of a few hundred places"), only copies and stage-2 buffer fills write. With `VICTORY_FILL_TITLE` on by default the small ring fills are mostly no longer taken here. The 4.6-8.6 ms "planning" is CPU planning (`does_of`, `plan`, `holder`, `mapped` page-table probes, 5.4k `constant_slot` writes), which a kernel would not touch. A real gather also needs a new PSO, a mapped upload table per `Own`, and a per-source/per-destination state scheme (NON_PIXEL_SHADER_RESOURCE/UAV instead of COPY_SOURCE/DEST, with the "both" case), and the scatter kernel in `send.rs` is one destination UAV per dispatch, so it is not reusable as is. Not done until a capture shows the command count. Done instead: runs touching in both buffers (reads may cross equal under-256-byte padding) go in one `CopyBufferRegion`; `VICTORY_JOIN_STEPS=0` restores one each. Measure with `--timing detailed` on the card-steps list.

R7 not done. Present cannot simply move: the next `present_planes` reads `GetCurrentBackBufferIndex` and records into that buffer, so it must come after the previous `Present` call, which makes the move a hand-off with a wait; Streamline's `present_begins/called/returned/ends` and the tags belong to one thread per frame (frame-gen semantics); and the resize path settles and calls `ResizeBuffers` under the presenter lock. What would be left to overlap is ~1.5 ms/frame (`Present`) plus `panel/menu/meter.frame`, which are bookkeeping (a once-a-second log). Not worth the ordering risk without an in-game Streamline test.
