#!/usr/bin/env python3
"""LLVM-assembled byte SAD controls on a D3D12 GPU.

v_msad_u8, v_qsad_pk_u16_u8, v_mqsad_pk_u16_u8 and v_mqsad_u32_u8 with
dynamic VGPR operands, including destinations that overlap the accumulator
or the S0 pair. The reference was checked against native RDNA2 execution
(tools/native_rdna2/check_sad.py): the masked forms skip a byte whose S1
byte is zero, and the 16-bit fields wrap.
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

M32 = 0xFFFFFFFF
SEED = 0x51534144
# v20:v21 is S0, v22 S1 and v24..v27 the accumulators.
INPUTS = [20, 21, 22, 24, 25, 26, 27]
INIT = [0x13572468 + 0x01010101 * k for k in range(len(INPUTS))]
# The guest's guarded view covers words 96..288: eight groups of 24 words.
GROUPS, STRIDE, START = 8, 24, 96


def sad(x, y, masked):
    total = 0
    for i in range(4):
        p, q = x >> 8 * i & 255, y >> 8 * i & 255
        if not (masked and q == 0):
            total += abs(p - q)
    return total


def reference(name, s0, s1, acc):
    """Result words for a 64-bit S0, a 32-bit S1 and accumulator words."""
    if name == "v_msad_u8":
        return [(sad(s0 & M32, s1, True) + acc[0]) & M32]
    masked = name.startswith("v_mqsad")
    sums = [sad(s0 >> 8 * k & M32, s1, masked) for k in range(4)]
    if name == "v_mqsad_u32_u8":
        return [(sums[k] + acc[k]) & M32 for k in range(4)]
    fields = [(sums[k] + (acc[k // 2] >> 16 * (k % 2) & 0xFFFF)) & 0xFFFF for k in range(4)]
    return [fields[0] | fields[1] << 16, fields[2] | fields[3] << 16]


def program():
    """The program and, per result, its instruction, first register and first accumulator."""
    lines = [f"v_mov_b32_e32 v{reg}, 0x{v:x}" for reg, v in zip(INPUTS, INIT)]
    lines += ["v_msad_u8 v30, v20, v22, v24",
              "v_qsad_pk_u16_u8 v[32:33], v[20:21], v22, v[24:25]",
              "v_mqsad_pk_u16_u8 v[34:35], v[20:21], v22, v[26:27]",
              "v_mqsad_u32_u8 v[36:39], v[20:21], v22, v[24:27]",
              # The destination is the accumulator.
              "v_mov_b32_e32 v40, v24", "v_mov_b32_e32 v41, v25",
              "v_qsad_pk_u16_u8 v[40:41], v[20:21], v22, v[40:41]",
              "v_mov_b32_e32 v44, v24", "v_mov_b32_e32 v45, v25", "v_mov_b32_e32 v46, v26", "v_mov_b32_e32 v47, v27",
              "v_mqsad_u32_u8 v[44:47], v[20:21], v22, v[44:47]",
              # The destination is the S0 pair.
              "v_mov_b32_e32 v48, v20", "v_mov_b32_e32 v49, v21",
              "v_mqsad_pk_u16_u8 v[48:49], v[48:49], v22, v[24:25]",
              "s_endpgm"]
    plan = [("v_msad_u8", 30, 0), ("v_qsad_pk_u16_u8", 32, 0), ("v_mqsad_pk_u16_u8", 34, 2),
            ("v_mqsad_u32_u8", 36, 0), ("v_qsad_pk_u16_u8", 40, 0), ("v_mqsad_u32_u8", 44, 0),
            ("v_mqsad_pk_u16_u8", 48, 0)]
    return lines, plan


def stored(plan):
    return [f"v{base + k}" for name, base, _ in plan for k in range(len(reference(name, 0, 0, [0] * 4)))]


def expected_outputs(vals, plan):
    s0, s1, acc = vals[0] | vals[1] << 32, vals[2], vals[3:]
    return [w for name, _, first in plan for w in reference(name, s0, s1, acc[first:])]


def instrument(text, names):
    marker = "[numthreads(1, 1, 1)]"
    if text.count(marker) != 1 or not text.rstrip().endswith("}"):
        raise ValueError("unexpected compute entry")
    text = text.replace(marker, "RWStructuredBuffer<uint> output : register(u0);\n" + marker)
    for j, (reg, v) in enumerate(zip(INPUTS, INIT)):
        text, found = re.subn(rf"\b(v{reg} = )0x{v:x}u;", rf"\g<1>output[{STRIDE}u * gid.x + {j}u];", text)
        if found != 1:
            raise ValueError(f"expected one initial literal for v{reg}, found {found}")
    stores = "".join(f"    output[{STRIDE}u * gid.x + {len(INIT) + i}u] = {name};\n" for i, name in enumerate(names))
    return text.rstrip()[:-1] + stores + "}\n"


def byte_word(rng, count):
    """Bytes that are often zero, equal, or at the extremes."""
    return sum(rng.choice([0, 0, 1, 0xFF, rng.getrandbits(8), rng.getrandbits(8)]) << 8 * i for i in range(count))


def inputs(rng, index):
    s0, s1 = byte_word(rng, 8), byte_word(rng, 4)
    if index % 5 == 0:
        s1 = s0 & M32  # the first window matches
    acc = [rng.choice([rng.getrandbits(32), 0xFFFFFFFF, 0xFFFEFFFD, 0xFC00FFF0, 0]) for _ in range(4)]
    return [s0 & M32, s0 >> 32, s1] + acc


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
    rng = random.Random(SEED)
    tool_hashes = {n: digest(getattr(a, n)) for n in ("emitter", "dxc", "dxv", "provider")}
    if a.baseline:
        tool_hashes["baseline"] = digest(a.baseline)
    lines, plan = program()
    names = stored(plan)
    if len(INIT) + len(names) != STRIDE:
        raise ValueError("stride does not match the outputs")
    words = assemble(lines, a.llvm_mc)
    if a.baseline:
        before = translate(a.baseline, words)
        if before.returncode != 65:
            raise ValueError(f"baseline did not refuse: {before.returncode} {before.stderr}")
    got = translate(a.emitter, words)
    got.check_returncode()
    source = instrument(got.stdout, names)
    rows, expected, negatives = [], {}, []
    with TemporaryDirectory(dir=a.work) as td:
        trial = Path(td)
        guest = ROOT / "tools/reference/independent-shaders/compute.cpp"
        (trial / "guest.cpp").write_text(guest.read_text())
        (trial / "context.h").write_text((guest.parent / "context.h").read_text().replace(
            '"../compute_buffers.h"', '"' + str(ROOT / "tools/reference/compute_buffers.h") + '"'))
        for k in range(a.dispatches):
            label = f"byte_sad_{k:02}"
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
                vals = inputs(rng, index)
                offset = START + STRIDE * lane
                seed[offset:offset + len(INIT)] = vals
                want[offset:offset + len(INIT)] = vals
                want[offset + len(INIT):offset + STRIDE] = expected_outputs(vals, plan)
            (trial / (label + ".seed")).write_bytes(struct.pack("<384I", *seed))
            expected[label] = struct.pack("<384I", *want)
            rows.append({"case": label, "dxil_sha256": digest(blob)})
        for kind, inst in [("clamp", ["v_qsad_pk_u16_u8 v[0:1], v[2:3], v4, v[6:7] clamp", "s_endpgm"]),
                           ("scalar_source", ["v_mqsad_u32_u8 v[0:3], s[2:3], v4, v[8:11]", "s_endpgm"]),
                           ("inline_accumulator", ["v_msad_u8 v0, v1, v2, 0", "s_endpgm"])]:
            bad = translate(a.emitter, assemble(inst, a.llvm_mc))
            if bad.returncode != 65:
                raise ValueError(f"unexpected acceptance {inst}")
            negatives.append({"kind": kind, "assembly": inst, "refusal": bad.stderr.strip()})
        (trial / "cases.txt").write_text("".join(f"{n} 4 structured {GROUPS}\n" for n in expected))
        exe, status, fields = run_gpu_guest(trial / "guest.cpp", "bytesad.exe", trial, a.work, a.proton,
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
        receipt = {"scope": "Synthetic LLVM-assembled v_msad_u8, v_qsad_pk_u16_u8, v_mqsad_pk_u16_u8 and v_mqsad_u32_u8 instructions; dynamic instrumented inputs and observed outputs",
                   "adapter": fields["adapter"], "proton": (a.proton / "version").read_text().strip(),
                   "seed": SEED, "assembly": lines, "native_words": [f"{v:08x}" for v in words],
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
