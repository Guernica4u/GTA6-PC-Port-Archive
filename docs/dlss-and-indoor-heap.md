# DLSS on NVIDIA, and what actually made the indoor rooms show up

Two separate problems were hiding behind "the indoor rooms are black and have no textures" on
an RTX 5070 Ti Laptop (12 GB, 32-lane waves) running the Xbox Series build. Neither of them was
a lighting parameter, and three days of turning `VICTORY_AMBIENT*` up and down found neither.

## 1. The textures: a descriptor heap that silently reads null

NVIDIA caps the shader-visible view heap at **1,000,000** slots. The title asks for **1,245,184**.
The layer kept only **500,000** of the title's indices reachable in place and rebuilt the rest into
dynamic slots, so roughly **245,000 bindings a frame read a null slot**. Outdoors the working set
fits under the reachable window; indoors — where the most distinct textures live — it does not, and
the surfaces come out untextured.

Measured with `VICTORY_HEAP_NULL_STATS=1`, which prints once every five seconds:

```
heap null: bindings_to_null=4387 slots_beyond_cap=… first_index=…
```

| `HEAP_REBALANCE` | bindings to null | frames with a refused table | fps @ 4,200 draws |
|---|---|---|---|
| off | 4,387 | 0 | 8.0 |
| **on** | **86** | 1 | 5.3 |

The picture is the point: with the switch on, the cell's cinder-block wall, the toilet rolls, the
mirror and the things on the desk all come back, and the whole room brightens because the surfaces
being lit now have albedo (`shots/heap-on2/frame-2849.png`).

`VICTORY_HEAP_REBALANCE=1` gives the title every index it can hold in place (about 737,855) and takes
the rest from the dynamic slots, which costs frames. `VICTORY_HEAP_INPLACE=<n>` is the middle ground:

| `HEAP_INPLACE` | in place | dynamic | bindings to null | fps |
|---|---|---|---|---|
| 500,000 | 500,000 | 499,999 | 5,871 | 8.0 |
| 600,000 | 600,000 | 399,999 | 5,094 | 8.0 |
| 700,000 | 700,000 | 299,999 | 5,561 | 7.8 |
| (rebalance) | 737,855 | small | **86** | 5.3–6.5 |

Only the full rebalance moves the number by an order of magnitude; the in-place knob alone is not
worth turning.

## 2. The darkness: the light stand-in, not its settings

The layer's own stand-in for the title's ray-traced indirect light has a clamp whose bounds are both
proportional to the local estimate:

```hlsl
found = clamp(found, 0.2 * all, 2.5 * all);          // both bounds scale with `all`
float top = max(dot(found, LUMA) * most, 1e-6);      // 0 when `all` is 0
was *= min(1.0, top / max(has, 1e-6));               // …so the title's own light is zeroed
```

In a dark room `all → 0`, so the estimate collapses to 0 and the title's own probe light is multiplied
away. Every multiplier above it (`VICTORY_AMBIENT` 0.25 … 1.5, `VICTORY_AMBIENT_MOST` 2 … 4) multiplies
zero. Only `VICTORY_AMBIENT=constant:<level>` escapes by returning early.

Two things follow, and both were measured on 175–345 captured frames each (mean luminance, share of
near-black pixels, share of blown-out pixels):

| configuration | mean | near-black | blown-out | verdict |
|---|---|---|---|---|
| old line, as shipped | 59.7 | 35.9% | 9.5% | dark |
| old line + heap rebalance | 61.2 | 22.6% | 7.4% | textures back, still dark |
| old line + `AMBIENT=1 MOST=4` | 96.4 | 4.6% | **14.1%** | the white haze people call "smoke" |
| **2026-11-26 line, `AMBIENT=0`, DLSS, frame gen 2x** | **75.6** | **5.2%** | **9.2%** | neither dark nor hazy |

The 2026-11-26 upstream turns the stand-in off (`VICTORY_AMBIENT=0`) and renders with CGBJ's renderer
instead; that combination is what a lit room actually looks like. An absolute floor on the estimate
(`VICTORY_AMBIENT_FLOOR=0.03`) changed nothing measurable (98.2 vs 96.0 mean, 16.2% vs 14.0% blown out),
so it is off by default and not recommended.

## 3. DLSS: the switch that does nothing without a console profile

DLSS is compiled in with `--features dlss` and needs NVIDIA's own files (`Get-NVIDIA-DLSS.cmd` fetches
about 70 MB from NVIDIA's GitHub into `nvidia-sdk/`). It also needs **`--console series-s`** on the
command line; without it the DLSS code is never called and the log says nothing at all about it, which
reads exactly like "DLSS is on but makes no difference". With it:

```
victory_gpu::dlss: DLSS: started; the title's upscale is DLSS (VICTORY_UPSCALE=dlss)
victory_gpu::dlss: DLSS: feature made input=(1280,720) output=(1920,1080) hdr=true depth_inverted=true
streamline: Streamline v2.14.1 …
```

Measured here: 9–10 rendered frames a second at 4,400–4,700 draws with the title rendering 720p and
DLSS taking it to 1080p, against 7–9 fps at render scale 0.4 on the older line — a sharper picture
*and* more frames. Frame generation (2x here) doubles what the screen shows; it and real ray tracing
are mutually exclusive in this build.

Building the MSVC target on a machine without Visual Studio works with `xwin` plus the `rust-lld`
that ships with the MSVC toolchain (0.6 GB of MSVC headers and libraries under `F:\xwin`), so DLSS
does not require installing Build Tools.

## What is still wrong

- Indoors stays warm and flat: the sun through the window dominates, and the shadow side has less
  detail than the retail game.
- A still camera occasionally shows one stale frame on top of the current one — the half-resolution
  picture and the title's temporal upscale, not something the layer can switch off (render scales
  0.75 and 1.0 both crash this build; only 0.5, 0.4 and 1/3 are usable).
- Frame generation leaves trails in cut scenes; the in-game F1 menu turns it off without a restart.

## 4. Frame generation and the DLSS upscale do not both run

Measured on the same build, same place, only `VICTORY_FRAME_GEN` changed:

| `VICTORY_FRAME_GEN` | `DLSS: started` | fps | picture |
|---|---|---|---|
| 1 (off) | yes | 9.3 | the title's 720p reconstructed to 1080p |
| 3 | **never** | 9.8 | the 720p picture stretched to 1080p |

So the smooth preset costs the upscale: with frame generation on, the layer's DLSS path is never entered
and the picture is stretched instead. The README warns only that ray tracing and frame generation do not
work together; this pairing is quieter — nothing fails, the picture is simply a little softer and the frame
counter looks the same. Two launchers are the honest answer: one sharp, one smooth.

## 5. A measurement lesson worth writing down

A run of comparisons late in the day "showed" that adding the window switches cost two thirds of the frame
rate — 9.5 fps down to 3.4. It did not. The comparison runs had been started with a short list of settings
and without the rest of the play set (`RELAY=2`, `CARD_STEPS=2`, `WIDE_BACK=1`, `CARD_REHASH_RUNS`,
`JOIN_COPIES`, `INDIRECT_WINDOW`, `RELAY_COPY`, `FETCH_CHECK`, `CARD_AWAKE`, `SHOW_AFTER`,
`PIPELINE_WAIT_MS`, `CARD_BUFFERS_*`). Without them the card sits at 90-100% and the layer manages 3-4 fps:

| settings | fps | GPU |
|---|---|---|
| short list | 3.4 | 89-100%, 2.5-2.7 GHz, 100-114 W |
| the full play set | 9.5 | — |
| full play set plus the window switches | 9.8 | — |

Any A/B has to carry the whole play set, or it measures the missing settings rather than the switch under test.

## 6. Where the frame goes now

Profiled at 9.4 fps (106 ms a frame) with DLSS and frame generation:

| part | time | data |
|---|---|---|
| preparing, copies | 7-34 ms | 60-125 MB |
| preparing, textures | 2-106 ms | 15-187 MB |
| on the GPU | 15-72 ms | — |
| copying back | 14-43 ms | 12-46 MB |

The card is the limit now, not the CPU: `VICTORY_HOIST_CLAMPS`, Belobug's change that took one scene from
225 ms to 72 ms of GPU time on AMD, measured **9.3 fps off against 8.8 fps on** here, because it removes
GPU work we are not short of. Of the CPU-side switches, only `VICTORY_JOIN_PARTS=1` helped (9.3 to 9.6);
`MAP_THREADS=0` (8.2) and `THIN=0` (9.0) were worse. 9.4-9.8 fps is the plateau on this card at 4,400 draws
with 720p and DLSS.