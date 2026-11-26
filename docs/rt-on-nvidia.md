# Running the ray-tracing structure builder on NVIDIA

*Measured 2026-11-27 on an RTX 4070 12 GB, Ryzen 5 7600X, 30 GiB, CachyOS Linux,
GE-Proton11-14 (vkd3d-proton 3.1-83), NVIDIA 580.196.07; Series S profile,
`VICTORY_RENDER_SCALE=0.5`, `VICTORY_OWN_UPSCALER=1`, `VICTORY_COMPOSE=1`.*

The README's point 14 says the builder cannot run on NVIDIA: its 64-lane programs are
refused, so every ray misses and the resolve passes make rooms white and hazy. The first
part stops being true with `VICTORY_FIRSTLANE_OWN=1`.

## Why

The builder has 53 programs. Without the switch, three of them fail to translate:

```
WARN victory_gpu::pipeline: the ray-tracing structure builder stays off: not all of its
programs have a host pipeline, and its stages run all or none missing=3 of=53
which=["39321a95223001ed", "77360df21e12ea43", "9fc111d3be79364f"]

WARN victory_gpu::pipeline: shader does not translate; its pipelines' draws are dropped
stage="compute" hash=39321a95223001ed error=converter shader.dxil v1: GFX10 code at
0x23c: a first-lane read where not every lane runs is not supported
(77360df21e12ea43: 0x23c; 9fc111d3be79364f: 0x258)
```

The switch is exact when the value is the same in every lane of the console wave, which
is what the builder's scans are (a sum of scalars inside a loop whose exit is per console
wave; see its comment in `lower.rs`). With it:

```
INFO victory_gpu::pipeline: the ray-tracing structure builder runs: all of its programs
have host pipelines pipelines=53
```

and 6–7 draws a frame are dropped instead of 500+.

## Settings

```
VICTORY_BVH_BUILDER=1
VICTORY_BVH_MISS=0
VICTORY_FIRSTLANE_OWN=1
# only the eight known driver-compiler hangs; NOT scripts/rt_off_hashes.txt
VICTORY_SKIP_PIPELINES=32ed072b9f845048,3b9d6c2ddaae83d8,3e7e04c03727b40c,479975ca9ff7e370,569d5818db7555f3,9fb1ebea2c1a32d4,a9c43aa1a29e7b32,fdd56d97a806bcca
```

## The cache trap

`VICTORY_FIRSTLANE_OWN` is part of the translation, so the shader cache key changes.
The pipeline library is keyed by shader hashes only, so old PSOs are served and the game
then translates hundreds of compute shaders while holding the last frame (in our run the
last frame was six minutes old with `shader translated` lines streaming). Before the run:

1. pre-translate with the same switches: `victory.exe shaders <shaders.rpf> <exe> ...`;
2. delete `pipelines/host-pipelines.bin`.

## Cost

| scene | fps | frame | GPU | draws |
|---|---|---|---|---|
| lighter | 11.3 | 88.1 ms | 34.7 ms | 2,654 |
| heavy | 7.8 | 128.6 ms | 62.1 ms | 4,612 |
| heavy (other spot) | 8.2–8.5 | 118–121 ms | 55–56 ms | 4.9–6.1k |

Builder off in the same build: 9.6 fps, but the resolve passes run over an empty BVH
(the white/hazy artifact). Correct RT costs about 1–2 fps here.

## Caveats

- One machine; Linux/Proton, 12 GB card, Series S at half scale; not measured on Windows.
- No AMD reference A/B for lighting correctness; the claim is that the builder runs and
  the RT path is live.
- VRAM is close to the ceiling at 0.5 scale (11.6 of 12.3 GB); 2/3 scale goes over it and
  the copies per frame triple.
