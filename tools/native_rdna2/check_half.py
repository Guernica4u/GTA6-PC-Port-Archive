#!/usr/bin/env python3
"""Compare the exact half-precision reference model with native RDNA2 execution.

Runs the f16 frexp and ldexp forms and the normalized 16-bit packs of
half.hip natively on a gfx1030 GPU under all 1024 MODE[9:0] values, derives
which MODE bits change any result, and compares the reference model of
tools/check_half_exact_gpu.py on every combination of those bits. Needs ROCm.
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
sys.path.insert(0, str(HERE))
from check_half_exact_gpu import SEED, inputs, reference
from check_model import build

OPS = ["v_frexp_mant_f16", "v_frexp_exp_i16_f16", "v_ldexp_f16", "v_cvt_pknorm_i16_f16", "v_cvt_pknorm_u16_f16",
       "v_cvt_pknorm_i16_f32", "v_cvt_pknorm_u16_f32"]
ALL_MODES = range(1024)


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
    code, host = build(a.rocm, a.work, HERE / "half.hip")
    rng = random.Random(SEED ^ 0x4e415456)
    rows = [inputs(rng, i) for i in range(a.records)]
    n = len(rows)
    report, total = [], 0
    for op in OPS:
        # half.hip reads S0 from f, S1 from c and the stale destination from the low word of b.
        native = [(s0, k if op == "v_ldexp_f16" else s1, old) for s0, s1, k, old in rows]
        records = a.work / f"{op}.in"
        records.write_bytes(b"".join(struct.pack("<QQII", 0, old, s1, s0) for s0, s1, old in native))
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
            for (s0, s1, old), hw in zip(native, struct.unpack(f"<{n}Q", per_mode[mode])):
                want = reference(op, s0, s1, old, mode)
                if hw != want:
                    mismatches += 1
                    if len(examples) < 8:
                        examples.append({"mode": f"{mode:03x}", "s0": f"{s0:08x}", "s1": f"{s1:08x}", "old": f"{old:08x}",
                                         "native": f"{hw:08x}", "model": f"{want:08x}"})
        total += len(modes) * n
        report.append({"instruction": op, "mode_bits_that_change_results": f"{influence:#05x}",
                       "modes_compared": len(modes), "comparisons": len(modes) * n, "mismatches": mismatches,
                       "examples": examples, "inputs_sha256": hashlib.sha256(records.read_bytes()).hexdigest(),
                       "native_results_sha256": hashlib.sha256(raw).hexdigest()})
        print(f"{op}: MODE bits {influence:#05x}, {len(modes)} modes, {mismatches} mismatches", flush=True)
    receipt = {"scope": "Native RDNA2 execution of synthetic inputs under all 1024 MODE[9:0] values, compared with the exact half-precision reference model",
               "device": device, "seed": SEED ^ 0x4e415456, "records": n,
               "kernels_sha256": hashlib.sha256((HERE / "half.hip").read_bytes()).hexdigest(),
               "native_runs": len(OPS) * len(ALL_MODES) * n, "model_comparisons": total,
               "mismatches": sum(r["mismatches"] for r in report), "instructions": report}
    with a.output.open("x") as f:
        json.dump(receipt, f, indent=2)
        f.write("\n")
    print(f"{'PASS' if not receipt['mismatches'] else 'FAIL'} {total} model comparisons, {receipt['mismatches']} mismatches")
    raise SystemExit(1 if receipt["mismatches"] else 0)


if __name__ == "__main__":
    main()
