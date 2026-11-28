# Experimental — VIctory Fork (ors33/victory-fork)

Everything below is experimental. Developed and measured on the maintainer's
machine; may not work on yours. Report regressions.

Base: VIctory-NVIDIA (`959dcaf`), itself on foxhound. Upstream `upstream/main`
= `6b3ce6c`, fully contained (0 behind).

## Changes vs upstream

### Ray tracing (on by default here; off upstream)
- Per-material resolve programs queued behind draw pipelines, not blocking builds.
- Ray page memory: 512 MB recycled budget, not 2048-page hard limit.
- Log line on any pipeline stalling a frame >= 50 ms.
- Hardware ray guide (`VICTORY_RAY_GUIDE`, off by default): card finds candidates;
  decoder soundness fix prevents hangs. Leaves objects out of rays' scene today.
- Water tile-slot fix (no dark patches, 32-lane cards).
- 64-bit compares on dwords (#1506); v_mad_u64_u32 carry on dwords (#1507).

### Memory / stability
- Texture-pool gate (`VICTORY_POOL_GATE`, on): keeps card under VRAM budget.
- Dynamic descriptor slots wait without submission lock (`VICTORY_DYNAMIC_WAIT=1`, on).
- Static sampler order fix removes ~8-minute recompile on start.
- `VICTORY_LEVELS_ON_DEMAND`: validated once (RTX 4070); off by default.

### Performance
- Ray-walker load reuse and cheaper traversal.
- `VICTORY_MEMORY_NEAR`, `VICTORY_LDS_START`, `VICTORY_WAVE_LOOP_SKIPS`.
- Measured wins promoted to defaults in `apps/victory/src/play.rs`.

### Shader translation
- Cache key includes all translator switches (#1504).
- Clippy clean (#1439).

### HLE / input / UI
- Password entries hide input (#1467).
- GameInput reading callbacks implemented (#1465).
- Audio DSP still stubbed (silent) — pending.

### CI / quality
- Clippy green (default + all-features, all-targets).
- Asset policy via pre-commit hook.

## Validation discipline

- One change per session, measured vs baseline at equal draw counts.
- Rollback on >5% fps regression or device loss.
- Switches promoted only after measured wins.
- `playability-resume.md` and `README.md` updated per change.

## Unproven

- Device losses on 8 GB / DLSS-FG (#1502, #1492); deterministic crashes (#1497, #1420).
- Ray-tracing guide picture incomplete; lime-green paint unidentified.
- HDR not implemented (SDR only).
- Occlusion queries fake (always visible).
- DSP reverb silent.
- Levels-on-demand not validated across scenes/cards.

See `playability-resume.md` for full priority list and metrics.
This fork is not affiliated with or endorsed by the title's developer, publisher, or NVIDIA.
See `docs/asset-policy.md`.