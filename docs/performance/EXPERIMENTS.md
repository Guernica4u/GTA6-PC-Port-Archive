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
