# VIctory Fork — Playability Resume

Reconciled with the tree at HEAD `7484752` on 2026-11-28 (220K lines of Rust across 242 source files, 9 crates) and cross-referenced with the [upstream issue tracker](https://github.com/fxceless/victory-fork/issues). Upstream audit: `upstream/main` is `6b3ce6c` and is fully contained in this fork (2 local docs commits ahead, 0 behind), so there are no new upstream commits to merge.

Recent changes: ray tracing is on by default (`9195780`); the texture-pool gate keeps the card under budget (`2eade68`); the static-sampler order removes the ~8-minute recompile (`9e90746`); the water's tile lists take a slot a lane (`cd93552`); and the hardware ray guide ran its first card session (`de57b01`, `6b3ce6c`).

## Current State: Playable, With Ray Tracing On

The game boots, runs menus at 30 fps, plays the opening cutscene fully lit, autosaves work, and the tester has played through the prologue and past the first boat mission. The fork is now based on VIctory-NVIDIA (`959dcaf`, 2026-11-27), itself built on foxhound, and the launcher starts the title with its own ray tracing on by default (`VICTORY_BVH_BUILDER=1`). In the current tree's measured runs on the tester's machine (i7-14700KF, RTX 4070 12 GB, 32 GB RAM) at 1920×1080, the open world runs at **~10–12.6 fps with ray tracing on** (about 12.6 at 5,000–7,000 draws, about 10 at 7,000–12,000); without ray tracing the same machine measured about 18 fps at 3,000–5,000 draws, 14 at 5,000–9,000 and 12.5 at 9,000–12,000 in an earlier session. The README's other test machine (RTX 5070, 2560×1440) reports 26–30 fps in the opening rooms; the scenes are not the same. The console target is 30 fps. **No device losses occurred in the current measured sessions**, but long sessions past the opening areas and community reports on some cards still need testing. Ray-traced light and shadows now render; ray tracing is still slow and incomplete.

---

## What Still Needs to Be Done

### Critical — Stability and Completeness

#### 1. GPU Device Loss (Not Reproduced in Current Runs, Still Reported)
The current tree's measured sessions report **no device losses or crashes** (README "Where it stands": RTX 5070, opening rooms; the RTX 4070 ray-tracing sessions in the README also completed). The earlier every-16-minutes device loss on the AMD line was traced to a texture slot shown before its upload had run; `VICTORY_SHOW_AFTER=1` is on by default and no loss was seen after it (likely fixed, not proven). The **30–50 minute crash on the tester's machine has not been re-confirmed since the Nov 26 rebase**; long sessions past the opening areas are still needed.

- The crash tracing (`VICTORY_D3D12_DRED=faults`) remains available in diagnostics builds and costs nothing measurable.
- Community reports on the current base remain open, mostly on 8 GB cards and with DLSS frame generation (listed below).
- One ray-tracing hang was found and fixed on the way to the hardware guide: the guided node test kept a word in group-shared memory that nothing stored; the software adapter zeroed it, but a card hung on the first walking dispatch. The program now stores it where it starts (`de57b01`).

**Related upstream issues:**
- [#1502](https://github.com/fxceless/victory-fork/issues/1502) — DLSS frame generation: device removed every 15–20 min on RTX 4060 Ti 8 GB; once took the display output (foxhound `2eade68c`)
- [#1492](https://github.com/fxceless/victory-fork/issues/1492) — RTX 3060 Laptop 6 GB: device removed (DEVICE_RESET, no driver event) every 8–10 min; GBV finds SRV dimension mismatches
- [#1497](https://github.com/fxceless/victory-fork/issues/1497) — Deterministic crash at +0x23cf683 on RTX 4060 8 GB / DDR4-2667
- [#1420](https://github.com/fxceless/victory-fork/issues/1420) — Regression after `52ac4ec`: title crashes ~1 min in (write to 0x0) on RTX 5080
- [#1267](https://github.com/fxceless/victory-fork/issues/1267) — RTX 4060 Laptop 8 GB: device loss at the cutscene before the prologue shootout
- [#1338](https://github.com/fxceless/victory-fork/issues/1338) — DISPATCH_SPREAD costs ~20 ms of card time on RTX 3060; with =0, 2 of 4 loads hang (DEVICE_HUNG)
- [#1259](https://github.com/fxceless/victory-fork/issues/1259) — RX 9070 XT (RDNA4): deterministic driver crash during pipeline compile

Remaining work: Re-test the 30–50 minute path on the current base; reproduce the 8 GB / DLSS-FG losses (#1502, #1492, #1267); check the deterministic crashes (#1497, #1420). The SRV dimension mismatches found by GBV (#1492) are still a strong lead.

#### 2. Ray Tracing Is On by Default, but Slow and Incomplete
Ray tracing is no longer off or non-functional: since 27 November the launcher sets `VICTORY_BVH_BUILDER=1`, the structure builder runs, and rooms receive their indirect light and lamp/sun shadows. The old "everything not in sunlight is solid black" state is gone when ray tracing is on. The cost is high: at 9,000 draws about 30 ms/frame goes to five ray-walking programs and about 40 ms to ordinary drawing, and water views cost more again.

- The title uses compute-shader-based ray tracing (not the DXR API), with hardware `image_bvh64_intersect_ray` instructions emulated in DXIL.
- **Builder:** the builder's programs translate (all 52 in the last ray-tracing census) and the builder runs by default (`4966cd6`, `9195780`). It is no longer gated off in real runs.
- **Trace programs:** 59 of 64 translate; **5 shadow-mask trace programs still do not translate** (a structure address picked from a descriptor by a computed index). The title did not create them in any run so far.
- **Hardware ray guide** (`VICTORY_RAY_GUIDE`, off by default; `de57b01`, `6b3ce6c`): first card session left about 590 of some 2,000 instances a frame out of the host top level (the soundness check turned down 3,598 of 4,007 bottom levels), so their ray-traced light and shadows were missing. Its 12.3–13.2 fps at 7,600–8,300 draws is **not a gain** until the picture is whole. Leave it off to play.
- **Known picture faults:** some vehicles' paint is flat lime green (a decal-layer data value, not a lighting fault); while travelling fast the ray programs' pages can lag behind.
- **Without ray tracing**, `VICTORY_AMBIENT` (0.8 in the launcher) stands in for indirect light: rooms are lit, but reflections are blurry and cutscenes can show the sky's colour.
- The water light runs on 32-lane cards with the per-lane tile-slot fix (`cd93552`); open water no longer has dark patches around boats, piers and lamp posts.

**Related upstream issues:**
- [#1356](https://github.com/fxceless/victory-fork/issues/1356) — Shadow cascade shader (`CS_ShadowRenderTiledCascadesAlphaCBPPPC`) now translates: stair-steps gone, but main character's head vanishes
- [#1310](https://github.com/fxceless/victory-fork/issues/1310) — DXR 1.1 for the `image_bvh_*` node tests (proposed alternative to current emulation)
- [#1257](https://github.com/fxceless/victory-fork/issues/1257) — old builder-start crash: "no number left for another copy" (COPIES=1023); the builder now runs by default, so re-test if it recurs
- [#1383](https://github.com/fxceless/victory-fork/issues/1383) — Shader size census of the 2.4× bloat + what another fork has for black shadows
- [#1459](https://github.com/fxceless/victory-fork/issues/1459) — NVIDIA: shadows and hair wrong in the prologue cell (picture depends on pipeline library fill order)
- [#1235](https://github.com/fxceless/victory-fork/issues/1235) — RTX 5080: dark interiors compared to AMD — likely shaders refused on 32-lane waves

Remaining work: Make the hardware guide's soundness check accept the title's scaled/moved static structures; translate the 5 shadow-mask programs if the title starts creating them; fix the lime-green paint input; keep ray pages resident at speed; re-measure the guide only when its picture is complete.

---

### Major — Severe Impact on Playability

#### 3. Frame Rate: ~10–12.6 fps With Ray Tracing (Target: 30 fps)
At 1920×1080 on the RTX 4070, the open world runs at about **12.6 fps at 5,000–7,000 draws and about 10 fps at 7,000–12,000 draws** with ray tracing on. Without ray tracing the same machine measured about 18 fps at 3,000–5,000 draws, 14 at 5,000–9,000 and 12.5 at 9,000–12,000 (earlier session). The card is the limit:

| Bottleneck | Current State |
|------------|---------------|
| GPU card time | ~75–100 ms/frame with ray tracing (target ~33 ms) |
| Ray-walking programs | ~30 ms/frame at 9,000 draws (five programs) |
| Ordinary drawing | ~40 ms/frame at 9,000 draws |
| Water light | extra in sea views |
| CPU submit thread | ~13 ms/frame busy; ~48 ms with ray tracing before `VICTORY_DYNAMIC_WAIT=1` (now on by default) |
| Translated shaders | last census (issue #1383) ~2.4× the console's instruction count; no newer census in the tree |
| Streaming | hitches in cutscenes while the title streams; the old ~490 ms camera-move stall is not re-measured on this base |

- **Shader optimization passes** (dead code, constant folding, value reuse) cut host instructions by ~43% on the earlier census, but the programs remain about 2.4× the console's.
- **169 `shortcut:` markers** in the codebase indicate approximations that may cost performance.
- The current defaults turn on the measured winners: `VICTORY_BVH_BUILDER=1`, `VICTORY_DYNAMIC_WAIT=1` and `VICTORY_WATER_LIGHT=1` from the launcher; `VICTORY_SAMPLER_ORDER=1` and `VICTORY_POOL_GATE=1` from the code.
- The sampler-order fix (`9e90746`) removed the ~8-minute 2–9 fps recompile on every start: compute pipelines over 1 s went 406 → 211 → 0 of 1,314, with all pipelines made 34 s after the start.

**Related upstream issues:**
- [#1510](https://github.com/fxceless/victory-fork/issues/1510) — Proposal: native SM 6.2 half/i16, SamplerDescriptorHeap/sharded PSO, and startup autotune
- [#1451](https://github.com/fxceless/victory-fork/issues/1451) — RTX 4080: LIGHT_REFRESH + SETTLE_ROOM gain ~20 ms/frame when submit thread is the limit
- [#1418](https://github.com/fxceless/victory-fork/issues/1418) — Gameplay progress: ~12 FPS on RTX 4090, stable for one hour
- [#1417](https://github.com/fxceless/victory-fork/issues/1417) — On an 8 GB card the GPU term halves when the translated loop bound halves
- [#1379](https://github.com/fxceless/victory-fork/issues/1379) — RTX 5080 Series S: 12.9 → ~25 fps in one day; what moved the frame and what didn't
- [#1349](https://github.com/fxceless/victory-fork/issues/1349) — Relay thread spends 50–70 ms/frame dropping ~75k retired objects (RTX 5080)
- [#1317](https://github.com/fxceless/victory-fork/issues/1317) — RTX 5080 Series S: seven CPU-side switches shorten the frame by ~30 ms (144→115 ms)
- [#1311](https://github.com/fxceless/victory-fork/issues/1311) — RTX 3080 Ti Series S: where the 97 ms go, what moved the frame and what didn't
- [#1268](https://github.com/fxceless/victory-fork/issues/1268) — RTX 4080 / Win10: **27.2 fps median** — reproducible NVIDIA recipe + PR stack
- [#1256](https://github.com/fxceless/victory-fork/issues/1256) — RTX 4090: ~70 ms of 153 ms frame is submit thread waiting (skinning passes' whole-heap declaration)
- [#1308](https://github.com/fxceless/victory-fork/issues/1308) — Proposal: RDNA2 → NIR → DXIL instead of direct lowering
- [#1278](https://github.com/fxceless/victory-fork/issues/1278) — RTX 4050 Laptop 6 GB: seven computes cost ~125 ms of GPU once 32-lane port runs them
- [#1238](https://github.com/fxceless/victory-fork/issues/1238) — watch::guard re-protection (~75 ms/frame): hot-block pinning + pipelined frame prep

Remaining work: Cut the ray-walking cost (the hardware guide once its picture is whole, cheaper traversal), reduce ordinary draw cost, and re-test the RTX 4080 recipe that reached 27 fps (#1268). The water light and memory gate are no longer the first-order blockers.

#### 4. Memory Pressure
The texture-pool gate (on by default; `VICTORY_POOL_GATE=0` reverts) keeps the card under its budget: in a 4.5-minute session the card used **8.1–8.7 GB against a ~10 GB budget**, with 163 blocks not made and no black or missing texture seen (`2eade68`). Textures live in 64 MB blocks that return to the card only when wholly empty. That fixes the earlier 0.8–1.0 GB overrun and ~160 ms frames with one-second stalls.

- `VICTORY_LEVELS_ON_DEMAND=1` has now run on a card (RTX 4070): **3.1 GB not allocated** for mip levels the title does not hold, no device loss. It is still **off by default** because it has not been validated across scenes and cards.
- The README requires **32 GB of system RAM** (64 GB comfortable); the earlier AMD line measured 40–46 GB private working set.
- DLSS frame generation needs VRAM headroom; the launcher holds back a fifth of the budget when frame generation is ready or on.
- Texture churn remains: the earlier line counted ~26,000 texture copies let go and ~3,400 remade in five minutes. The gate stops over-budget growth but does not eliminate the churn.

**Related upstream issues:**
- [#1488](https://github.com/fxceless/victory-fork/issues/1488) — RTX 4090 + 64 GB RAM: host memory pressure at world entry (~40 GiB working set) and 7–8 fps
- [#1500](https://github.com/fxceless/victory-fork/issues/1500) — Add memory-pressure safeguards, timing safety and frame-time testing
- [#1477](https://github.com/fxceless/victory-fork/issues/1477) — 8 GiB RTX 5070 Laptop: reference state, measurements, and settings that remove the world
- [#1382](https://github.com/fxceless/victory-fork/issues/1382) — Performance & Memory Suggestions (Unified Memory / PCIe overhead, 8 GB VRAM support)
- [#1284](https://github.com/fxceless/victory-fork/issues/1284) — Textures flash black on a small PC: 98,064 slots "found unmapped and made null" in 13 min
- [#1423](https://github.com/fxceless/victory-fork/issues/1423) — Proposal: upload textures by what shaders sampled, let go of least recently sampled (RX 9070 XT)
- [#1438](https://github.com/fxceless/victory-fork/issues/1438) — GPU memory: repeated page writes can indefinitely postpone delayed pull-back
- [#1244](https://github.com/fxceless/victory-fork/issues/1244) — Memory accounting at world entry on NVIDIA (RTX 4060 Laptop, 32 GB)

Remaining work: Validate levels-on-demand across scenes and cards, then decide whether to turn it on; reduce the textures-let-go-and-remade cycle; evaluate buffer placement strategies; implement memory safeguards (#1500).

#### 5. Shader Translation Coverage Gaps
No current shader census is published in the tree. The last published census ([docs/README-fxceless.md](docs/README-fxceless.md), before the Nov 26 rebase) reported:

| Category | Translated | Total | Gap |
|----------|-----------|-------|-----|
| Mesh shaders | 8,765 | 8,765 | 0 |
| Vertex/Pixel | 12,550 | 12,592 | 42 |
| Compute | 2,579 | 2,637 | 58 |

Those numbers predate the current base and the Nov 27 translator work, so treat them as historical rather than a current measurement. Known remaining gaps:
- **Merged geometry stages** (combined vertex+geometry): 195 programs counted separately in the last census.
- **64-lane wave programs** running on 32-lane hardware; the builder's programs now translate, but some compute programs still need 64-lane waves.
- **Five shadow-mask ray-trace programs** (see §2).
- **Shader cache key** still misses four translator switches (`WATER_TILE_SLOTS`, `WAVE_LOOP_SKIPS`, `MEMORY_NEAR`, `LDS_START`), so a warm cache can serve the wrong translation (#1504).
- Untranslated shaders still cause their **draws to be silently dropped** — parts of the scene simply don't render.
- The old **64-bit image atomic** gap is closed: the translator now emits `INT64` / `ATOMIC_TYPED64` forms for wide image atomics.

**Related upstream issues:**
- [#1507](https://github.com/fxceless/victory-fork/issues/1507) — Shaders: the carry of `v_mad_u64_u32` on its dwords (wrong on AMD)
- [#1506](https://github.com/fxceless/victory-fork/issues/1506) — Shaders: 64-bit integer compares on their dwords (backport of upstream VIctory#1599)
- [#1504](https://github.com/fxceless/victory-fork/issues/1504) — Shader cache key misses four translator switches — warm cache serves wrong translations
- [#1243](https://github.com/fxceless/victory-fork/issues/1243) — Console thread IDs in 2D/3D compute groups on 32-lane hosts: wave-aligned rounding doesn't hold
- [#1439](https://github.com/fxceless/victory-fork/issues/1439) — CI: cargo clippy fails on main in victory-shader
- [#1337](https://github.com/fxceless/victory-fork/issues/1337) — emit::rect tests fail with Windows SDK 22621 DXIL validator (PSV0 mismatch)
- [#1343](https://github.com/fxceless/victory-fork/issues/1343) — victory-shader's test host crashes on NVIDIA: empty root-constant list

Remaining work: Publish a fresh census on the current base; translate merged geometry stages and the remaining 64-lane programs; fix the shader cache key (#1504).

---

### Moderate — Noticeable but Not Blocking Core Gameplay

#### 6. GPU Queries Are Fake
From [query.rs](crates/victory-gpu/src/query.rs#L1-L2):
> *"Nothing runs on a GPU yet, so no query ever produces a result"*

- Occlusion queries always return "visible"; binary occlusion reports visible; pipeline statistics return plausible fixed values; timestamps are the submission's length.
- This means no occlusion culling — the GPU draws everything, contributing to the frame rate problem.

Remaining work: Implement real occlusion queries on the host GPU for proper culling.

#### 7. Depth Bounds Implemented; Comparison Codes Still Approximated
The depth bounds test now works (#82, `c96b4f2`): a pipeline that enables it is made with a `DEPTH_STENCIL1` stream and the title's `OMSetDepthBounds` reaches the host list. On a host GPU without support the draws pass at every depth (logged). The comparison codes for depth/stencil are still approximated:
> *"shortcut: the comparison codes are taken to be the comparison"* ([list.rs](crates/victory-gpu/src/list.rs#L2068))

Remaining work: Implement exact depth/stencil comparison codes.

#### 8. HDR Missing; Resolution Handled by Launcher Patches
HDR output is still not done: [display.rs](crates/victory-hle/src/modules/display.rs#L61) reports "HDR mode: disabled (SDR presentation)" and the swap chain is SDR. Resolution is handled by the launcher's image patches (`--res 1440`, `1080to1440`, `4k`), not by the reported display mode: `VICTORY_DISPLAY_MODE` defaults to 3840×2160 and is only a console-style report; the title may ignore it.

Remaining work: Implement HDR tone mapping/output; derive the reported mode from the host display or confirm the title honors it.

#### 9. Audio: Reverb and Phone Calls Are Silent
From [dsp.rs](crates/victory-hle/src/modules/dsp.rs#L14):
> *"Outputs are silence, so reverb returns are dry."*

- XMA voice decoding and Opus streams work.
- DSP effects (reverb, convolution) are **stubbed to output silence**.
- Phone call audio is silent (only one APU decoder connected).

**Related upstream issues:**
- [#1491](https://github.com/fxceless/victory-fork/issues/1491) — Audio DSP: compute mono convolutions, so TVs, phone calls and reverb are heard
- [#1470](https://github.com/fxceless/victory-fork/issues/1470) — Phone call audio is silent (only one APU decoder connected)
- [#1509](https://github.com/fxceless/victory-fork/issues/1509) — Audio: the XMA decoder runs at time-critical priority

Remaining work: Implement DSP reverb/convolution processing (#1491); connect additional APU decoders (#1470).

#### 10. Visual Bugs
Current known:
- Some vehicles' paint is flat lime green (a decal-layer paint value picked through a mask texture; not a lighting fault).
- While travelling fast, the ray programs' pages can lag behind.
- The hardware ray guide leaves objects out of the rays' scene (`VICTORY_RAY_GUIDE`, off by default; see §2).
- The first room's walls can be missing on the first shot in some launches (see §14).
- The water's dark patches around boats, piers and lamp posts are fixed by the per-lane tile-slot change (`cd93552`); water light is on by default.
- Foam on the water is no longer listed as dark in the current README; the water tile-slot work landed at the same time.

**Related upstream issues:**
- [#1368](https://github.com/fxceless/victory-fork/issues/1368) — NVIDIA RTX 4070 SUPER: under water the screen is black with star-shaped glare
- [#1319](https://github.com/fxceless/victory-fork/issues/1319) — NVIDIA RTX 4070 SUPER: picture breaks into black blocks and white haze with clear sky
- [#1345](https://github.com/fxceless/victory-fork/issues/1345) — NVIDIA GTX 1070: missing textures on Pascal
- [#1223](https://github.com/fxceless/victory-fork/issues/1223) — `VICTORY_FILLS_PLAIN=1`: dark smear on cell wall in 3 of 4 launches (RTX 5080)
- [#1401](https://github.com/fxceless/victory-fork/issues/1401) — RTX 3060 Ti: water patches fixed (paired form), packed-texture face noise
- [#1301](https://github.com/fxceless/victory-fork/issues/1301) — Linux/Proton: stretched character meshes

#### 11. Render-Target Metadata Planes Not Supported
From [texture.rs](crates/victory-formats/src/texture.rs#L873):
> *"shortcut: no compression metadata planes (HTILE, CMASK, FMASK, DCC)"*

Remaining work: Implement HTILE/CMASK/FMASK/DCC metadata plane support for correct render-target handling.

---

### Minor / Polish — Not Blocking Playability

#### 12. Series S Profile: Menus Fixed, World Not Validated
The launcher offers `-SeriesS` (1280×720 inside), and the foxhound base fixed menus, loading screens and subtitles on Series S (they were black). The current README's measurements are Series X only; the Series S world was black in earlier runs and is **not validated on the current base**. ([#1379](https://github.com/fxceless/victory-fork/issues/1379) reports 12.9 → ~25 fps on RTX 5080 with Series S.)

#### 13. Input and UI Gaps
- Trigger rumble (Xbox impulse triggers) not implemented: the code drops trigger motors because XInput has none and WinRT `Gamepad.Vibration` bindings are unavailable outside diagnostics builds.
- Password text entry doesn't hide input ([#1467](https://github.com/fxceless/victory-fork/issues/1467)).
- GameInput: reading callbacks never run ([#1465](https://github.com/fxceless/victory-fork/issues/1465)); system-button registration takes five arguments where the platform expects six ([#1462](https://github.com/fxceless/victory-fork/issues/1462)).

#### 14. Intermittent First-Room Rendering Bug
> *"The interior of the first gameplay room is sometimes not drawn after the cutscene hands over to gameplay."*

The first room's walls now appear in later frames of 4 of 4 runs (2 of 5 before the fix), but the very first shot can still come up without walls in some launches — a timing or memory-sync race remains.

#### 15. Frame Pacing Drift
> *"Frame pacing drifts on one screen in release builds"* (menus)

See [#1347](https://github.com/fxceless/victory-fork/issues/1347) — Measure frame pacing when asynchronous Present unblocks submission.

#### 16. Linux/Proton Support
- [#1490](https://github.com/fxceless/victory-fork/issues/1490) — Linux/Proton fork: final state, no longer maintained
- [#1489](https://github.com/fxceless/victory-fork/issues/1489) — GPU: restore indirect and card-copy shaders under Proton
- [#1486](https://github.com/fxceless/victory-fork/issues/1486) — GPU: initialize Series S compositor on Proton
- [#1375](https://github.com/fxceless/victory-fork/issues/1375) — Linux/Wine RX 7900 XTX: submit thread waits 64 ms/frame for submission lock
- [#1340](https://github.com/fxceless/victory-fork/issues/1340) — `VICTORY_TEXTURE_BACK=1` runs the driver out of DMA mappings on Linux/NVIDIA

#### 17. CI and Code Quality
- [#1439](https://github.com/fxceless/victory-fork/issues/1439) — CI clippy fails on main in victory-shader
- [#1342](https://github.com/fxceless/victory-fork/issues/1342) — Asset policy's 512 KiB limit exceeded by host.rs and lower.rs; CI is red
- [#1505](https://github.com/fxceless/victory-fork/issues/1505) — Add exhaustive development plan + agent rules

#### 18. HLE Interface Gaps
Many Xbox runtime interfaces have `missing::` stubs (not called by this title but would affect others):
- Speech-to-text, closed captions, high contrast accessibility
- Message dialogs, send invite, achievements, player picker, error dialogs
- Some GameInput methods (`QueryInterface`, dispatcher, aggregate device)
- Diagnostic screenshot/clip capture

---

## Priority Order

1. Re-test device losses on the current base (#1502, #1492, #1497, #1267).
2. Complete ray tracing: guide soundness, the five shadow-mask programs, lime-green paint.
3. Raise the frame rate to 30 fps by cutting ray-walker and draw cost (#1268, #1451).
4. Validate the memory gate and levels-on-demand (#1488, #1500, #1423).
5. Publish a fresh shader census and fix the cache key (#1504, #1507, #1506).
6. Implement real occlusion queries and audio (#1491, #1470).
7. Add HDR and fix visual bugs (#1368, #1319, #1345).
8. Polish: Series S, rumble, Proton, CI.

## Key Metrics

| Metric | Current | Target |
|--------|---------|--------|
| World FPS | ~10–12.6 with ray tracing on (RTX 4070, 1080p); ~18/14/12.5 without RT in an earlier session | 30 |
| Session stability | No device losses in current measured runs; long sessions past opening areas and 8 GB / DLSS-FG reports still open | No crashes |
| Shader coverage | Last census (pre-rebase): mesh 8,765/8,765, VS/PS 12,550/12,592, compute 2,579/2,637; plus 195 merged-geometry programs | 100% |
| Shadow lighting | Lit with ray tracing on; hardware guide and 5 shadow-mask programs incomplete | Complete ray-traced lighting |
| RAM requirement | 32 GB required, 64 GB comfortable (40–46 GB private on the earlier AMD line) | Fits in 16–32 GB comfortably |
| VRAM usage | 8.1–8.7 GB of a ~10 GB budget with the gate | ≤8 GB with headroom |
| `shortcut:` approximations | 169 | Reduce to correctness-critical only |
| Open upstream issues | 300 (2026-11-28) | Triage and address by priority |
| Env switches (`VICTORY_*`) | 449 distinct names; 92 set by the launcher | Promote measured winners to defaults |

## Upstream Issue References

There are **300 open issues** in the upstream tracker as of 2026-11-28. The table lists the ones that map to this resume; the counts are per category, not the whole tracker.

| Category | Count | Key Issues |
|----------|-------|------------|
| GPU device loss / crashes | 7 | #1502, #1492, #1497, #1420, #1267, #1338, #1259 |
| Shadows / ray tracing | 6 | #1356, #1310, #1257, #1383, #1459, #1235 |
| Frame rate / performance | 13 | #1510, #1451, #1418, #1268, #1256, #1349, #1317, #1311, #1278, #1238, #1417, #1379, #1308 |
| Memory pressure | 8 | #1488, #1500, #1477, #1382, #1284, #1423, #1438, #1244 |
| Shader translation | 7 | #1507, #1506, #1504, #1243, #1439, #1337, #1343 |
| Audio | 3 | #1491, #1470, #1509 |
| Visual bugs | 6 | #1368, #1319, #1345, #1223, #1401, #1301 |
| Linux/Proton | 5 | #1490, #1489, #1486, #1375, #1340 |
| Input / UI | 3 | #1467, #1465, #1462 |
| CI / code quality | 3 | #1439, #1342, #1505 |
| Proposals / diagnostics | 8 | #1510, #1355, #1348, #1346, #1309, #1307, #1306, #1305 |
