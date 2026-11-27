#!/usr/bin/env python3
"""Compare the byte SAD reference model with native RDNA2 execution.

Runs v_msad_u8, v_qsad_pk_u16_u8, v_mqsad_pk_u16_u8 and v_mqsad_u32_u8
natively on a gfx1030 GPU and compares the reference of
tools/check_byte_sad_gpu.py. Each kernel runs under MODE 0 and 0x3ff to
show that MODE does not change these integer results. Needs ROCm.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
from check_byte_sad_gpu import SEED, inputs, reference
from check_model import build

M32 = 0xFFFFFFFF
# Kernel name, instruction, and the result words the kernel stores.
KERNELS = [("v_msad_u8", "v_msad_u8", slice(0, 1)), ("v_qsad_pk_u16_u8", "v_qsad_pk_u16_u8", slice(0, 2)),
           ("v_mqsad_pk_u16_u8", "v_mqsad_pk_u16_u8", slice(0, 2)),
           ("v_mqsad_u32_u8_0", "v_mqsad_u32_u8", slice(0, 2)), ("v_mqsad_u32_u8_1", "v_mqsad_u32_u8", slice(2, 4))]
MODES = [0, 0x3FF]


def record(vals):
    """kernels.hip reads S0 = a, S1 = c, accumulators (b, f, low ^ high word of b)."""
    s0, s1, acc = vals[0] | vals[1] << 32, vals[2], vals[3:6]
    acc = acc + [acc[0] ^ acc[1]]
    return (s0, acc[0] | acc[1] << 32, s1, acc[2]), (s0, s1, acc)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--rocm", type=Path, default=Path("/opt/rocm"))
    ap.add_argument("--work", type=Path, required=True)
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument("--records", type=int, default=16384, help="input records, a multiple of 64")
    a = ap.parse_args()
    if a.output.exists():
        ap.error("choose a fresh receipt")
    a.work.mkdir(parents=True, exist_ok=False)
    code, host = build(a.rocm, a.work, HERE / "sad.hip")
    rng = random.Random(SEED ^ 0x4e415456)
    rows = [record(inputs(rng, i)) for i in range(a.records)]
    records = a.work / "inputs.bin"
    records.write_bytes(b"".join(struct.pack("<QQII", *native) for native, _ in rows))
    n = len(rows)
    report = []
    for kernel, name, words in KERNELS:
        results = a.work / f"{kernel}.bin"
        run = subprocess.run([host, code, records, results, kernel] + [str(m) for m in MODES],
                             check=True, capture_output=True, text=True)
        device = run.stdout.strip()
        raw = results.read_bytes()
        mode_independent = raw[:8 * n] == raw[8 * n:]
        mismatches, examples = 0, []
        for (native, model), hw in zip(rows, struct.unpack(f"<{n}Q", raw[:8 * n])):
            out = reference(name, *model)[words]
            want = out[0] | (out[1] << 32 if len(out) > 1 else 0)
            if hw != want:
                mismatches += 1
                if len(examples) < 8:
                    examples.append({"s0": f"{native[0]:016x}", "s1": f"{native[2]:08x}",
                                     "acc": [f"{w:08x}" for w in model[2]], "native": f"{hw:016x}", "model": f"{want:016x}"})
        report.append({"kernel": kernel, "instruction": name, "comparisons": n, "mismatches": mismatches,
                       "mode_independent": mode_independent, "examples": examples,
                       "native_results_sha256": hashlib.sha256(raw).hexdigest()})
        print(f"{kernel}: {mismatches} mismatches, MODE-independent {mode_independent}", flush=True)
    failed = sum(r["mismatches"] for r in report) or not all(r["mode_independent"] for r in report)
    receipt = {"scope": "Native RDNA2 execution of synthetic byte SAD inputs under MODE 0 and 0x3ff, compared with the byte SAD reference model",
               "device": device, "seed": SEED ^ 0x4e415456, "records": n, "modes": [f"{m:#05x}" for m in MODES],
               "inputs_sha256": hashlib.sha256(records.read_bytes()).hexdigest(),
               "kernels_sha256": hashlib.sha256((HERE / "sad.hip").read_bytes()).hexdigest(),
               "model_comparisons": n * len(KERNELS), "mismatches": sum(r["mismatches"] for r in report),
               "kernels": report}
    with a.output.open("x") as f:
        json.dump(receipt, f, indent=2)
        f.write("\n")
    print(f"{'FAIL' if failed else 'PASS'} {receipt['model_comparisons']} model comparisons, {receipt['mismatches']} mismatches")
    raise SystemExit(1 if failed else 0)


if __name__ == "__main__":
    main()
