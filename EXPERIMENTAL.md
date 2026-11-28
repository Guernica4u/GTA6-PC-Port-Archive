# Experimental Changes — VIctory Fork (ors33/victory-fork)

> **Status: EXPERIMENTAL.** This fork is a work in progress. Every change
> listed below was developed and measured on the maintainer's own machine and
> may not work on your hardware. Nothing here is guaranteed stable. Test on
> your own setup before relying on it, and report regressions.

This file sits at the repository root (like `INSTALL.md`) so it is easy to find.
It details every change that differs from the upstream bases
(`trustno1-coder/VIctory-NVIDIA`, built on `Marvisak/VIctory` / foxhound),
and marks each as experimental.

## Base

- Based on VIctory-NVIDIA (`959dcaf`, 2026-11-27), itself built on foxhound.
- Ray-tracing fixes and the fork's own additions are on top.
- Upstream audit: `upstream/main` is `6b3ce6c`, fully contained in this fork
  (0 behind, docs commits ahead).

## Experimental changes by area

### Ray tracing (on by default here; off upstream)
- Per-material ray-tracing resolve programs no longer waited for while the
  driver builds them; they queue behind draw pipelines.
- Ray program memory pages are a 512 MB recycled budget, not a 2,048-page
  hard limit.
- Log line names any pipeline that stalls a frame >= 50 ms.
- Hardware ray guide (`VICTORY_RAY_GUIDE`, off by default): card finds
  candidates; decoder soundness fix prevents device hangs. **Experimental —
  leaves objects out of the rays' scene today.**
- Water tile-slot fix (no dark patches on 32-lane cards).
- 64-bit integer compares run on their dwords (AMD correctness, #1506).
- `v_mad_u64_u32` carry computed on dwords (AMD correctness, #1507).

### Memory / stability
- Texture-pool gate (`VICTORY_POOL_GATE`, on by default): keeps card under
  its VRAM budget; textures wait instead of overrunning.
- Dynamic descriptor slots wait without the submission lock
  (`VICTORY_DYNAMIC_WAIT=1`, on by default).
- Static sampler order fix removes ~8-minute recompile on every start.
- `VICTORY_LEVELS_ON_DEMAND` validated once (RTX 4070); off by default.

### Performance
- Ray-walker load reuse and cheaper traversal.
- `VICTORY_MEMORY_NEAR`, `VICTORY_LDS_START`, `VICTORY_WAVE_LOOP_SKIPS`.
- Measured wins promoted to defaults in `apps/victory/src/play.rs`.

### Shader translation
- Shader cache key now includes all translator switches (#1504).
- Clippy clean in `victory-shader` (#1439).

### HLE / input / UI
- Password text entries hide input (#1467).
- GameInput reading callbacks implemented (#1465).
- Audio DSP still stubbed (reverb/convolution silent) — pending.

### CI / code quality
- Clippy green in both default and all-features configurations.
- Asset policy enforced via pre-commit hook.

## How changes are validated (no mistakes)

- One change per session, measured against a baseline at equal draw counts.
- Rollback on >5% fps regression or any device loss.
- Switches promoted to defaults only after measured wins.
- `playability-resume.md` and `README.md` updated per change.

## Known experimental / unproven items

- Device losses on 8 GB cards and with DLSS frame generation (#1502, #1492).
- Deterministic crashes (#1497, #1420).
- Ray-tracing guide picture incomplete.
- Lime-green vehicle paint (decal layer input not identified).
- HDR output not implemented (SDR only).
- Real occlusion queries not implemented (fake: always visible).
- DSP reverb/convolution silent.
- Levels-on-demand not validated across scenes/cards.

## Status of each

Marked experimental until validated on the tester's machine
(i7-14700KF, RTX 4070 12 GB, 32 GB RAM, 1920x1080) and across community
cards. See `playability-resume.md` for the full priority list and metrics.

This fork is **not** affiliated with or endorsed by the title's developer,
publisher, or NVIDIA. See `docs/asset-policy.md`.