#!/usr/bin/env python3
"""LLVM-assembled quad-mask, bit-replicate and saveexec controls on a D3D12 GPU.

s_quadmask_b32/b64 and s_bitreplicate_b64_b32 on ordinary scalar values,
including SCC and one in-place form, and s_nor/xnor/orn1_saveexec_b32/b64.
Each saveexec form starts from a dynamic EXEC value and records the saved
mask, the new mask, SCC, and whether the invocation's lane stays active.
The reference follows the RDNA2 ISA pseudocode on whole registers; the
translator represents EXEC per invocation with the lane in bit 0.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import re
import struct
import subprocess
import sys
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from check_packed_integer_gpu import assemble, digest, translate
from frosty_gpu_guest import run_gpu_guest

# s4:s5 is the operand, s6:s7 the starting EXEC value.
INIT = (0x13572468, 0x24681357, 0x35792468, 0x46813579)
SAVEEXEC = ("s_nor_saveexec", "s_xnor_saveexec", "s_orn1_saveexec")
# The guest's guarded view covers words 96..288: four groups of 46 words.
GROUPS, STRIDE, START = 4, 46, 96
EDGE = [0, 0xffffffff, 1, 0x80000000, 0x11111111, 0x88888888, 0x0000000f, 0xf0000000,
        0x00010000, 0x0000fffe, 0x7ffffffe, 0x00000002]
M32, M64 = 0xffffffff, (1 << 64) - 1


def quadmask(x, quads):
    return sum(1 << k for k in range(quads) if x >> 4 * k & 15)


def bitreplicate(x):
    return sum(3 << 2 * k for k in range(32) if x >> k & 1)


def combine(name, src, exec_, mask):
    return {"s_nor_saveexec": ~(src | exec_), "s_xnor_saveexec": ~(src ^ exec_),
            "s_orn1_saveexec": ~src | exec_}[name] & mask


def program():
    """The program and the registers stored after it, in order."""
    lines = [f"s_mov_b32 s{4 + j}, 0x{v:x}" for j, v in enumerate(INIT)]
    lines += [f"v_mov_b32_e32 v{10 + k}, 0" for k in range(6)]
    lines += ["s_quadmask_b32 s16, s4", "s_cselect_b32 s17, 1, 0",
              "s_quadmask_b64 s[18:19], s[4:5]", "s_cselect_b32 s20, 1, 0",
              # SCC from the compare must survive s_bitreplicate.
              "s_cmp_eq_u32 s4, s5", "s_bitreplicate_b64_b32 s[22:23], s4", "s_cselect_b32 s21, 1, 0",
              "s_bitreplicate_b64_b32 s[24:25], s5",
              "s_mov_b64 s[26:27], s[4:5]", "s_quadmask_b64 s[26:27], s[26:27]"]
    stored = [f"s{n}" for n in range(16, 28)]
    reg = 28
    for k, name in enumerate(SAVEEXEC):
        lines += ["s_mov_b32 exec_lo, s6", f"{name}_b32 s{reg}, s4", f"s_cselect_b32 s{reg + 1}, 1, 0",
                  f"s_mov_b32 s{reg + 2}, exec_lo", f"v_mov_b32_e32 v{10 + k}, 1", f"s_mov_b32 exec_lo, s{reg}"]
        stored += [f"s{reg}", f"s{reg + 1}", f"s{reg + 2}"]
        reg += 3
    reg = 38  # register pairs start at an even register
    for k, name in enumerate(SAVEEXEC):
        lines += ["s_mov_b64 exec, s[6:7]", f"{name}_b64 s[{reg}:{reg + 1}], s[4:5]",
                  f"s_cselect_b32 s{reg + 4}, 1, 0", f"s_mov_b64 s[{reg + 2}:{reg + 3}], exec",
                  f"v_mov_b32_e32 v{13 + k}, 1", f"s_mov_b64 exec, s[{reg}:{reg + 1}]"]
        stored += [f"s{reg}", f"s{reg + 1}", f"s{reg + 4}", f"s{reg + 2}", f"s{reg + 3}"]
        reg += 6
    return lines + ["s_endpgm"], stored + [f"v{10 + k}" for k in range(6)]


def expected_outputs(s4, s5, s6, s7):
    src64, exec64 = s4 | s5 << 32, s6 | s7 << 32
    out = [quadmask(s4, 8), int(quadmask(s4, 8) != 0), quadmask(src64, 16), 0, int(quadmask(src64, 16) != 0)]
    rep4, rep5 = bitreplicate(s4), bitreplicate(s5)
    out += [int(s4 == s5), rep4 & M32, rep4 >> 32, rep5 & M32, rep5 >> 32, quadmask(src64, 16), 0]
    lanes = []
    for wide in (False, True):
        for name in SAVEEXEC:
            if wide:
                new = combine(name, src64, exec64, M64)
                out += [s6, s7, int(new != 0), new & M32, new >> 32]
            else:
                # The b32 forms leave EXEC_HI unchanged.
                new = combine(name, s4, s6, M32)
                out += [s6, int(new != 0), new]
            lanes.append(new & 1)
    return out + lanes


def instrument(text, stored):
    marker = "[numthreads(1, 1, 1)]"
    if text.count(marker) != 1 or not text.rstrip().endswith("}"):
        raise ValueError("unexpected compute entry")
    text = text.replace(marker, "RWStructuredBuffer<uint> output : register(u0);\n" + marker)
    for j, v in enumerate(INIT):
        text, found = re.subn(rf"\b(s{4 + j} = )0x{v:x}u;", rf"\g<1>output[{STRIDE}u * gid.x + {j}u];", text)
        if found != 1:
            raise ValueError(f"expected one initial literal for s{4 + j}, found {found}")
    stores = "".join(f"    output[{STRIDE}u * gid.x + {len(INIT) + i}u] = {name};\n"
                     for i, name in enumerate(stored))
    return text.rstrip()[:-1] + stores + "}\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for n in ("emitter", "llvm-mc", "dxc", "dxv", "provider", "proton", "prefix", "compiler-dir", "work", "output"):
        ap.add_argument("--" + n, type=Path, required=True)
    ap.add_argument("--baseline", type=Path, help="earlier emitter that must refuse the program")
    ap.add_argument("--adapter", required=True)
    ap.add_argument("--dispatches", type=int, default=48)
    a = ap.parse_args()
    if a.output.exists():
        ap.error("choose a fresh receipt")
    a.work.mkdir(parents=True, exist_ok=False)
    rng = random.Random(0x53514d4b)
    tool_hashes = {n: digest(getattr(a, n)) for n in ("emitter", "dxc", "dxv", "provider")}
    if a.baseline:
        tool_hashes["baseline"] = digest(a.baseline)
    lines, stored = program()
    if len(INIT) + len(stored) != STRIDE:
        raise ValueError("stride does not match the outputs")
    words = assemble(lines, a.llvm_mc)
    if a.baseline:
        before = translate(a.baseline, words)
        if before.returncode != 65:
            raise ValueError(f"baseline did not refuse: {before.returncode} {before.stderr}")
    got = translate(a.emitter, words)
    got.check_returncode()
    source = instrument(got.stdout, stored)
    rows, expected, negatives = [], {}, []
    with TemporaryDirectory(dir=a.work) as td:
        trial = Path(td)
        guest = ROOT / "tools/reference/independent-shaders/compute.cpp"
        (trial / "guest.cpp").write_text(guest.read_text())
        (trial / "context.h").write_text((guest.parent / "context.h").read_text().replace(
            '"../compute_buffers.h"', '"' + str(ROOT / "tools/reference/compute_buffers.h") + '"'))
        for k in range(a.dispatches):
            label = f"scalar_masks_{k:02}"
            shader = trial / (label + ".hlsl")
            shader.write_text(source)
            blob = shader.with_suffix(".cso")
            subprocess.run([a.dxc, "-T", "cs_6_0", "-E", "main", "-O3", "-Fo", blob, shader],
                           check=True, capture_output=True, timeout=60)
            subprocess.run([a.dxv, blob], check=True, capture_output=True, timeout=15)
            seed = [rng.getrandbits(32) for _ in range(384)]
            want = seed.copy()
            for lane in range(GROUPS):
                index = k * GROUPS + lane
                if index < 2 * len(EDGE):
                    vals = [EDGE[(index + 3 * j) % len(EDGE)] for j in range(4)]
                elif index % 3 == 0:
                    vals = [rng.getrandbits(32) for _ in range(4)]
                    vals[1] = vals[0]  # equal operand words for the SCC compare
                else:
                    # Sparse values leave some quads empty.
                    vals = [rng.getrandbits(32) & rng.getrandbits(32) & rng.getrandbits(32) for _ in range(4)]
                offset = START + STRIDE * lane
                seed[offset:offset + 4] = vals
                want[offset:offset + 4] = vals
                want[offset + 4:offset + STRIDE] = expected_outputs(*vals)
            (trial / (label + ".seed")).write_bytes(struct.pack("<384I", *seed))
            expected[label] = struct.pack("<384I", *want)
            rows.append({"case": label, "dxil_sha256": digest(blob)})
        for kind, inst in [("wave_mask_source", ["s_quadmask_b64 s[8:9], exec", "s_endpgm"]),
                           ("compare_mask_source", ["v_cmp_eq_u32_e32 vcc_lo, 0, v1", "s_quadmask_b32 s8, vcc_lo", "s_endpgm"]),
                           ("unproven_source", ["s_bitreplicate_b64_b32 s[8:9], s3", "s_endpgm"])]:
            bad = translate(a.emitter, assemble(inst, a.llvm_mc))
            if bad.returncode != 65:
                raise ValueError(f"unexpected acceptance {inst}")
            negatives.append({"kind": kind, "assembly": inst, "refusal": bad.stderr.strip()})
        (trial / "cases.txt").write_text("".join(f"{n} 4 structured {GROUPS}\n" for n in expected))
        exe, status, fields = run_gpu_guest(trial / "guest.cpp", "scalarmasks.exe", trial, a.work, a.proton,
                                            a.prefix, a.provider, a.compiler_dir)
        if status or fields.get("adapter") != a.adapter:
            raise RuntimeError(f"guest failed: {status} {fields}")
        failures = []
        for row in rows:
            actual = trial / (row["case"] + ".bin")
            observed = actual.read_bytes() if actual.is_file() else b""
            if observed != expected[row["case"]]:
                lhs = struct.unpack(f"<{len(observed) // 4}I", observed)
                rhs = struct.unpack("<384I", expected[row["case"]])
                failures.append({"case": row["case"], "different_words": [
                    (i, f"{x:08x}", f"{y:08x}") for i, (x, y) in enumerate(zip(lhs, rhs)) if x != y][:16]})
            else:
                row["readback_sha256"] = digest(actual)
        receipt = {"scope": "Synthetic LLVM-assembled s_quadmask, s_bitreplicate and nor/xnor/orn1 saveexec instructions; dynamic instrumented inputs and observed outputs",
                   "adapter": fields["adapter"], "proton": (a.proton / "version").read_text().strip(),
                   "seed": 0x53514d4b, "assembly": lines, "native_words": [f"{v:08x}" for v in words],
                   "emitted_sha256": hashlib.sha256(got.stdout.encode()).hexdigest(),
                   "shader_cases": len(rows), "inputs": len(rows) * GROUPS,
                   "result_words": len(rows) * GROUPS * (STRIDE - len(INIT)), "compared_words": 384 * len(rows),
                   "mismatches": len(failures), "failures": failures,
                   "baseline_refused": bool(a.baseline), "negative_controls": negatives,
                   "tool_sha256": tool_hashes, "guest_sha256": digest(exe), "rows": rows}
    with a.output.open("x") as f:
        json.dump(receipt, f, indent=2)
        f.write("\n")
    if failures:
        print(f"FAIL {len(failures)} of {len(rows)} dispatches; first: {failures[0]}")
        raise SystemExit(1)
    print(f"PASS {receipt['inputs']} inputs, {receipt['result_words']} results, {receipt['compared_words']} GPU words; zero mismatches")


if __name__ == "__main__":
    main()
