#!/usr/bin/env python3
"""Compare the f64-modes reference model with native RDNA2 execution.

Builds kernels.hip and host.c, runs each instruction natively on a gfx1030
GPU under all 1024 MODE[9:0] values, derives which MODE bits change any
result, and compares the reference model of tools/check_f64_modes_gpu.py
on every combination of those bits. Needs ROCm (hipcc and the HSA runtime).
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import struct
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from check_f64_modes_gpu import SEED, inputs, reference

OPS = ["v_cvt_f64_f32", "v_cvt_f32_f64", "v_min_f64", "v_max_f64", "v_frexp_exp_i32_f64", "v_frexp_mant_f64",
       "v_fract_f64", "v_ldexp_f64", "v_frexp_exp_i32_f32", "v_frexp_mant_f32",
       "v_trunc_f64", "v_floor_f64", "v_ceil_f64", "v_rndne_f64"]
ALL_MODES = range(1024)


def build(rocm, work):
    bundle, code, host = work / "kernels.hsaco", work / "kernels.co", work / "host"
    subprocess.run([rocm / "bin/hipcc", "--offload-arch=gfx1030", "-O2", "--genco", HERE / "kernels.hip", "-o", bundle],
                   check=True, capture_output=True)
    subprocess.run([rocm / "llvm/bin/clang-offload-bundler", "--unbundle", "--type=o", f"--input={bundle}",
                    "--targets=hipv4-amdgcn-amd-amdhsa--gfx1030", f"--output={code}"], check=True)
    subprocess.run(["cc", "-O2", HERE / "host.c", f"-I{rocm}/include", f"-L{rocm}/lib", f"-Wl,-rpath,{rocm}/lib",
                    "-lhsa-runtime64", "-o", host], check=True)
    return code, host


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--rocm", type=Path, default=Path("/opt/rocm"))
    ap.add_argument("--work", type=Path, required=True)
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument("--records", type=int, default=4096, help="input records, a multiple of 64")
    a = ap.parse_args()
    if a.output.exists():
        ap.error("choose a fresh receipt")
    a.work.mkdir(parents=True, exist_ok=False)
    code, host = build(a.rocm, a.work)
    rng = random.Random(SEED ^ 0x4e415456)
    rows = [inputs(rng, i) for i in range(a.records)]
    records = a.work / "inputs.bin"
    records.write_bytes(b"".join(struct.pack("<QQII", x, y, c, f) for x, y, c, f in rows))
    n = len(rows)
    report, total = [], 0
    for op in OPS:
        results = a.work / f"{op}.bin"
        run = subprocess.run([host, code, records, results, op] + [str(m) for m in ALL_MODES],
                             check=True, capture_output=True, text=True)
        device = run.stdout.strip()
        raw = results.read_bytes()
        per_mode = [raw[8 * n * m:8 * n * (m + 1)] for m in ALL_MODES]
        bits = [b for b in range(10) if any(per_mode[m] != per_mode[m | 1 << b] for m in ALL_MODES if not m >> b & 1)]
        influence = sum(1 << b for b in bits)
        mismatches, examples = 0, []
        modes = [sum(c) for r in range(len(bits) + 1) for c in itertools.combinations([1 << b for b in bits], r)]
        for mode in modes:
            observed = struct.unpack(f"<{n}Q", per_mode[mode])
            for (x, y, c, f), hw in zip(rows, observed):
                want = reference(op, x, y, c, f, mode)
                if hw != want:
                    mismatches += 1
                    if len(examples) < 8:
                        examples.append({"mode": f"{mode:03x}", "a": f"{x:016x}", "b": f"{y:016x}", "c": f"{c:08x}",
                                         "f": f"{f:08x}", "native": f"{hw:016x}", "model": f"{want:016x}"})
        total += len(modes) * n
        report.append({"instruction": op, "mode_bits_that_change_results": f"{influence:#05x}",
                       "modes_compared": len(modes), "comparisons": len(modes) * n, "mismatches": mismatches,
                       "examples": examples, "native_results_sha256": hashlib.sha256(raw).hexdigest()})
        print(f"{op}: MODE bits {influence:#05x}, {len(modes)} modes, {mismatches} mismatches", flush=True)
    receipt = {"scope": "Native RDNA2 execution of synthetic inputs under all 1024 MODE[9:0] values, compared with the f64-modes reference model",
               "device": device, "seed": SEED ^ 0x4e415456, "records": n,
               "inputs_sha256": hashlib.sha256(records.read_bytes()).hexdigest(),
               "kernels_sha256": hashlib.sha256((HERE / "kernels.hip").read_bytes()).hexdigest(),
               "native_runs": len(OPS) * len(ALL_MODES) * n, "model_comparisons": total,
               "mismatches": sum(r["mismatches"] for r in report), "instructions": report}
    with a.output.open("x") as f:
        json.dump(receipt, f, indent=2)
        f.write("\n")
    print(f"{'PASS' if not receipt['mismatches'] else 'FAIL'} {total} model comparisons, {receipt['mismatches']} mismatches")
    raise SystemExit(1 if receipt["mismatches"] else 0)


if __name__ == "__main__":
    main()
