# Shader harness

[`tools/shader_harness.py`](../tools/shader_harness.py) tests a change to the [shader translator](../tools/shader-compiler) against a title's recorded shaders and against your graphics cards. It compares your working tree with a baseline, `origin/main` unless you name another, and says what blocks the change.

```sh
python tools/shader_harness.py run
```

It needs Linux, a folder of recorded shader requests from your own copy of a title, and the DXC used for [shader preparation](PROJECTSCARLETT.md). The GPU stages also need what [the full check](../CONTRIBUTING.md#the-full-check) needs.

## Configuration

Put the paths in `build/shader-harness.json`, which Git ignores:

```json
{
  "requests": "PREFIX/drive_c/users/steamuser/AppData/Local/Temp/tuxbox_shader_cache/projectscarlett/HASH/requests",
  "dxc": "DXC/bin/dxc",
  "proton": "PROTON",
  "windows_dxc": "DXC-WINDOWS/bin/x64",
  "adapters": [
    {"name": "nvidia", "id": "10de:XXXX", "prefix": "PREFIX-A"},
    {"name": "amd", "id": "1002:XXXX", "prefix": "PREFIX-B",
     "env": {"VKD3D_FILTER_DEVICE_NAME": "RADV", "DXVK_FILTER_DEVICE_NAME": "RADV"}}
  ]
}
```

## Stages

| Stage | What it does | Blocks when |
| --- | --- | --- |
| Code checks | `tuxbox.py check --no-gpu` | any step fails |
| Survey | Translates every request with both builds. Compute programs are translated twice, for hosts with 64-lane and with 32-lane waves | a program stops translating, or the translator crashes or hangs |
| Determinism | Translates a sample again | any outcome or output differs |
| Compile | Runs changed and recovered programs through DXC and DXV | a program that compiled no longer does |
| GPU | Builds, then runs the [translator checks](../tools/check_translator_gpu.py) and the shader [Lanes](../examples/lanes/README.md) suites on each card | any check fails |
| Coverage | Lists instructions the recorded shaders use that no GPU check program holds, and the largest refusals | never; it shows where to add tests |

The report is `report.md` in the run folder under `build/shader-harness/`, with request names and details in `report.json`. Neither holds shader bytes or HLSL. It names any of AMD and NVIDIA that did not run.

Options for common cases:

| Option | Use |
| --- | --- |
| `--baseline REF` | Compare with another commit |
| `--identical` | A refactor: any change of translated output blocks |
| `--no-gpu` | CPU stages only. The run still fails unless you add `--allow-no-gpu` |
| `--compile-limit 0` | Compile every changed and recovered program, not a sample of 200 of each |

Single stages: `survey --xshader BUILD --output FILE`, `compare BEFORE AFTER`, and `explain --xshader BUILD REQUEST` for one program on both hosts.

## The instruction inventory

[INSTRUCTIONS.md](INSTRUCTIONS.md) lists every RDNA 2 instruction with what the translator does with it alone, which cards a GPU check ran it on, and how many recorded shaders use it. It is generated from three files in the repository, and the Rust and Python tests fail when any of them is out of date.

| You changed | Run |
| --- | --- |
| What the translator does with an instruction | `TUXBOX_BLESS_INSTRUCTIONS=1 cargo test --manifest-path tools/shader-compiler/Cargo.toml --target x86_64-unknown-linux-gnu instruction_inventory`, then `python tools/shader_harness.py inventory` |
| A GPU check, or the recorded requests | `python tools/shader_harness.py run`, then `python tools/shader_harness.py inventory --from-run RUN-FOLDER` |

The second saves the run's instruction names and counts under `evidence/shader-instructions/`. It needs `"title"` in the configuration, such as `"ProjectScarlett 1.0.2.1"`, and a passing run of a committed tree on every card you list. Commit the record, the evidence and the document together.

The translator must refuse an instruction it can't translate. The inventory's test fails if any instruction makes it crash.

## What a pass does not show

Recorded shaders are translated and compiled, but **not executed**. A changed output that compiles can still compute something wrong. The GPU checks run synthetic programs: when you add or fix an instruction, add it to one of them, and use the coverage table to find instructions that have none. A check program holding an instruction does not prove every operand form or lane state was tested. The harness does not run or time the drivers' own pipeline builds.
