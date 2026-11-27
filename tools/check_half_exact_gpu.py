#!/usr/bin/env python3
"""LLVM-assembled exact half-precision and normalized-pack controls on a D3D12 GPU.

v_frexp_mant_f16, v_frexp_exp_i16_f16, v_ldexp_f16, v_cvt_pknorm_i16_f16,
v_cvt_pknorm_u16_f16 and v_cvt_pknorm_i16_f32, plus the existing
v_cvt_pknorm_u16_f32, under eleven MODE values written with
s_setreg_imm32_b32. Destinations start with a dynamic stale value, so the
results show which destination bits each form keeps. The exact rational
reference was compared with native RDNA2 execution under all 1024 MODE values
(tools/native_rdna2/check_half.py).
"""
import argparse
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import subprocess
import sys
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from check_packed_integer_gpu import assemble, digest, load_input, translate
from frosty_gpu_guest import run_gpu_guest

# v4 and v5 are the operands, v6 the ldexp exponent, v7 the stale destination value.
INIT = (0x13572468, 0x24681357, 0x35792468, 0x46813579)
# (instruction, destination register, sources)
FORMS = [("v_frexp_mant_f16", 10, "v4"), ("v_frexp_exp_i16_f16", 11, "v4"), ("v_ldexp_f16", 12, "v4, v6"),
         ("v_cvt_pknorm_i16_f16", 13, "v4, v5"), ("v_cvt_pknorm_u16_f16", 14, "v4, v5"),
         ("v_cvt_pknorm_i16_f32", 15, "v4, v5"), ("v_cvt_pknorm_u16_f32", 16, "v4, v5")]
VOP3_ONLY = ("v_cvt_pknorm_i16_f16", "v_cvt_pknorm_u16_f16", "v_cvt_pknorm_i16_f32", "v_cvt_pknorm_u16_f32")
OUTPUTS = 9  # v10..v18, including the aliased frexp and ldexp forms in v17 and v18
# The guest's guarded view covers words 96..288: fourteen groups of 13 words.
GROUPS, STRIDE, START = 14, 13, 96
# MODE[9:0]: f32 round [1:0], f64/f16 round [3:2], f32 denormals [5:4],
# f64/f16 denormals [7:6], DX10 clamp [8], IEEE [9]. None keeps the default 0xc0.
MODES = [None, 0x000, 0x005, 0x04a, 0x0f5, 0x0fa, 0x0ff, 0x2b0, 0x2c0, 0x2cf, 0x3ff]
SEED = 0x48414c46
M16 = 0xFFFF


# ----- Reference -----

def half_value(h):
    e, m = h >> 10 & 31, h & 1023
    v = Q(m, 1 << 24) if e == 0 else Q(1024 + m, 1 << 25) * (1 << e)
    return -v if h & 0x8000 else v


def f32_value(x):
    e, m = x >> 23 & 255, x & 0x7FFFFF
    v = Q(m, 1 << 149) if e == 0 else Q(0x800000 + m, 1 << 23) * Q(2) ** (e - 127)
    return -v if x >> 31 else v


def round_magnitude(q, up, down):
    """Round a nonnegative Fraction to an integer: nearest even, or up or down."""
    n = math.floor(q)
    if q == n or down:
        return n
    if up:
        return n + 1
    return n + 1 if q - n > Q(1, 2) or (q - n == Q(1, 2) and n % 2) else n


def encode_half(v, sign, mode):
    """Exact binary16 rounding of a positive magnitude, tininess after rounding."""
    rmode = mode >> 2 & 3
    up = (rmode == 1 and not sign) or (rmode == 2 and sign)
    down = rmode == 3 or (rmode == 1 and sign) or (rmode == 2 and not sign)
    e = v.numerator.bit_length() - v.denominator.bit_length()
    if Q(2) ** e > v:
        e -= 1
    n = round_magnitude(v / Q(2) ** (e - 10), up, down)
    if n == 2048:
        n, e = 1024, e + 1
    s = sign << 15
    if e < -14:
        return s if not mode & 0x80 else s | round_magnitude(v * (1 << 24), up, down)
    if e > 15:
        return s | (0x7BFF if down else 0x7C00)
    return s | (e + 15) << 10 | (n - 1024)


def frexp_mant(h, mode):
    s, e, m = h & 0x8000, h >> 10 & 31, h & 1023
    if e == 0 and not mode & 0x40:
        return s
    if e == 31:
        return h | 0x200 if m and mode & 0x200 else h
    if e == 0 and m == 0:
        return h
    if e == 0:
        m = (m << (11 - m.bit_length())) & 1023
    return s | 0x3800 | m


def frexp_exp(h, mode):
    e, m = h >> 10 & 31, h & 1023
    if e == 31 or (e == 0 and (m == 0 or not mode & 0x40)):
        return 0
    return (m.bit_length() - 24 if e == 0 else e - 14) & M16


def ldexp(h, k, mode):
    s, e, m = h & 0x8000, h >> 10 & 31, h & 1023
    if e == 31:
        return h | 0x200 if m and mode & 0x200 else h
    if e == 0 and (m == 0 or not mode & 0x40):
        return s
    k = (k & M16) - (1 << 16 if k & 0x8000 else 0)  # the low 16 bits, signed
    return encode_half(abs(half_value(h)) * Q(2) ** k, s >> 15, mode)


def norm_half(h, mode, signed):
    if h & 0x7FFF > 0x7C00 or (h & 0x8000 and not signed) or (h & 0x7C00 == 0 and not mode & 0x40):
        return 0
    r = round_magnitude(min(abs(half_value(h)), Q(1)) * (32767 if signed else 65535), False, False)
    return (-r if h & 0x8000 else r) & M16


def norm_f32(x, signed):
    if x & 0x7FFFFFFF > 0x7F800000 or (x >> 31 and not signed):
        return 0
    v = min(abs(f32_value(x)) if x & 0x7FFFFFFF != 0x7F800000 else Q(1), Q(1))
    r = round_magnitude(v * (32767 if signed else 65535), False, False)
    return (-r if x >> 31 else r) & M16


def reference(name, s0, s1, old, mode):
    """The destination word after `name` with sources s0, s1 and a stale destination."""
    keep = old & 0xFFFF0000
    if name == "v_frexp_mant_f16":
        return keep | frexp_mant(s0 & M16, mode)
    if name == "v_frexp_exp_i16_f16":
        return keep | frexp_exp(s0 & M16, mode)
    if name == "v_ldexp_f16":
        return keep | ldexp(s0 & M16, s1, mode)
    signed = "_i16_" in name
    if name.endswith("_f16"):
        return norm_half(s0 & M16, mode, signed) | norm_half(s1 & M16, mode, signed) << 16
    return norm_f32(s0, signed) | norm_f32(s1, signed) << 16


def effective(mode):
    return 0xc0 if mode is None else mode


# ----- Inputs -----

def fb(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


EDGE_F16 = [0, 0x8000, 1, 0x8001, 0x03FF, 0x83FF, 0x0400, 0x8400, 0x3C00, 0xBC00, 0x3BFF, 0xBBFF, 0x3C01,
            0x7BFF, 0xFBFF, 0x7C00, 0xFC00, 0x7E00, 0xFE00, 0x7C01, 0x7D00, 0xFC01, 0x3800, 0x0200, 0x0155]
EDGE_F32 = [0, 0x8000_0000, 1, 0x8000_0001, 0x007F_FFFF, 0x3F80_0000, 0xBF80_0000, 0x3F7F_FFFF, 0xBF7F_FFFF,
            0x3F80_0001, 0x7F80_0000, 0xFF80_0000, 0x7FC0_0000, 0x7F80_0001, 0xFFC0_0001, fb(0.5), fb(-0.5),
            fb(0.5 / 32767), fb(-0.5 / 32767), fb(0.5 / 65535), fb(1.5 / 65535), fb(32766.5 / 32767)]
EDGE_EXP = [0, 1, -1, 15, -14, -24, -25, 16, 29, 30, -40, 0x7FFF, 0x8000, 0xFFFF, 0x1_0003, 0xFFFF_0002,
            0x8000_0000, 0x7FFF_FFFF]


def random_half(rng):
    e = rng.choice([0, 31, 1, 30, 15, 14, 13]) if rng.random() < 0.4 else rng.getrandbits(5)
    return rng.getrandbits(1) << 15 | e << 10 | rng.getrandbits(10)


def random_f32(rng):
    if rng.random() < 0.35:
        # Near a rounding midpoint of x * 32767 or x * 65535.
        scale = rng.choice([32767, 65535])
        x = fb((rng.randrange(scale) + 0.5) / scale) + rng.choice([-1, 0, 0, 1])
    elif rng.random() < 0.5:
        x = rng.getrandbits(1) << 31 | rng.randrange(0x3000_0000, 0x3F80_0000)
    else:
        x = rng.getrandbits(1) << 31 | (rng.choice([0, 255, 126, 127, 128]) if rng.random() < 0.3 else rng.getrandbits(8)) << 23 | rng.getrandbits(23)
    return x ^ rng.getrandbits(1) << 31


def inputs(rng, index):
    """(s0, s1, exponent, stale destination) for one lane; even lanes hold halves, odd lanes f32 values."""
    if index % 2 == 0:
        i = index // 2
        lo = EDGE_F16[i % len(EDGE_F16)] if i < 4 * len(EDGE_F16) else random_half(rng)
        hi = EDGE_F16[(i * 7 + 3) % len(EDGE_F16)] if i < 4 * len(EDGE_F16) else random_half(rng)
        s0, s1 = rng.getrandbits(16) << 16 | lo, rng.getrandbits(16) << 16 | hi
    else:
        i = index // 2
        s0 = EDGE_F32[i % len(EDGE_F32)] if i < 2 * len(EDGE_F32) else random_f32(rng)
        s1 = random_f32(rng) if rng.random() < 0.8 else rng.choice(EDGE_F32)
    k = EDGE_EXP[index % len(EDGE_EXP)] if rng.random() < 0.4 else rng.choice(
        [rng.randint(-40, 40), rng.getrandbits(32)])
    return s0, s1, k % 2**32, rng.getrandbits(32)


# ----- Program -----

def program(mode, e64):
    lines = [f"v_mov_b32_e32 v{4 + j}, 0x{v:x}" for j, v in enumerate(INIT)]
    if mode is not None:
        lines.append(f"s_setreg_imm32_b32 hwreg(HW_REG_MODE, 0, 10), 0x{mode:x}")
    # Stale destination values from v7.
    lines += [f"v_mov_b32_e32 v{dst}, v7" for _, dst, _ in FORMS]
    for name, dst, sources in FORMS:
        suffix = "" if name in VOP3_ONLY else "_e64" if e64 else "_e32"
        lines.append(f"{name}{suffix} v{dst}, {sources}")
    e = "_e64" if e64 else "_e32"
    # The destination is the source.
    lines += ["v_mov_b32_e32 v17, v4", f"v_frexp_mant_f16{e} v17, v17",
              "v_mov_b32_e32 v18, v4", f"v_ldexp_f16{e} v18, v18, v6", "s_endpgm"]
    return lines


def expected_outputs(s0, s1, k, old, mode):
    out = [reference(name, s0, k if name == "v_ldexp_f16" else s1, old, mode) for name, _, _ in FORMS]
    return out + [reference("v_frexp_mant_f16", s0, 0, s0, mode), reference("v_ldexp_f16", s0, k, s0, mode)]


def instrument(text):
    marker = "[numthreads(1, 1, 1)]"
    if text.count(marker) != 1 or not text.rstrip().endswith("}"):
        raise ValueError("unexpected compute entry")
    text = text.replace(marker, "RWStructuredBuffer<uint> output : register(u0);\n" + marker)
    for j, v in enumerate(INIT):
        text = load_input(text, 4 + j, v, f"output[{STRIDE}u * gid.x + {j}u]")
    stores = "".join(f"    output[{STRIDE}u * gid.x + {len(INIT) + i}u] = v{10 + i};\n" for i in range(OUTPUTS))
    return text.rstrip()[:-1] + stores + "}\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for n in ("emitter", "llvm-mc", "dxc", "dxv", "provider", "proton", "prefix", "compiler-dir", "work", "output"):
        ap.add_argument("--" + n, type=Path, required=True)
    ap.add_argument("--baseline", type=Path, help="earlier emitter that must refuse every program")
    ap.add_argument("--adapter", required=True)
    ap.add_argument("--dispatches", type=int, default=24, help="dispatches per variant, fourteen inputs each")
    a = ap.parse_args()
    if a.output.exists():
        ap.error("choose a fresh receipt")
    a.work.mkdir(parents=True, exist_ok=False)
    rng = random.Random(SEED)
    tool_hashes = {n: digest(getattr(a, n)) for n in ("emitter", "dxc", "dxv", "provider")}
    if a.baseline:
        tool_hashes["baseline"] = digest(a.baseline)
    rows, programs, expected, negatives = [], [], {}, []
    variants = [(mode, False) for mode in MODES] + [(None, True), (0x2cf, True)]
    with TemporaryDirectory(dir=a.work) as td:
        trial = Path(td)
        guest = ROOT / "tools/reference/independent-shaders/compute.cpp"
        (trial / "guest.cpp").write_text(guest.read_text())
        (trial / "context.h").write_text((guest.parent / "context.h").read_text().replace(
            '"../compute_buffers.h"', '"' + str(ROOT / "tools/reference/compute_buffers.h") + '"'))
        inputs_used = 0
        for mode, e64 in variants:
            lines = program(mode, e64)
            words = assemble(lines, a.llvm_mc)
            if a.baseline:
                before = translate(a.baseline, words)
                if before.returncode != 65:
                    raise ValueError(f"baseline did not refuse: {before.returncode} {before.stderr}")
            got = translate(a.emitter, words)
            got.check_returncode()
            source = instrument(got.stdout)
            tag = f"{'default' if mode is None else f'mode{mode:03x}'}{'_e64' if e64 else ''}"
            programs.append({"program": tag, "mode": effective(mode), "mode_written": mode is not None, "e64": e64,
                             "native_words": [f"{v:08x}" for v in words],
                             "emitted_sha256": hashlib.sha256(got.stdout.encode()).hexdigest()})
            for k in range(a.dispatches):
                label = f"halfexact_{tag}_{k:02}"
                shader = trial / (label + ".hlsl")
                shader.write_text(source)
                blob = shader.with_suffix(".cso")
                subprocess.run([a.dxc, "-T", "cs_6_0", "-E", "main", "-O3", "-Fo", blob, shader],
                               check=True, capture_output=True, timeout=120)
                subprocess.run([a.dxv, blob], check=True, capture_output=True, timeout=15)
                seed = [rng.getrandbits(32) for _ in range(384)]
                want = seed.copy()
                for lane in range(GROUPS):
                    x, y, c, f = inputs(rng, k * GROUPS + lane)
                    offset = START + STRIDE * lane
                    seed[offset:offset + len(INIT)] = [x, y, c, f]
                    want[offset:offset + len(INIT)] = seed[offset:offset + len(INIT)]
                    want[offset + len(INIT):offset + STRIDE] = expected_outputs(x, y, c, f, effective(mode))
                    inputs_used += 1
                (trial / (label + ".seed")).write_bytes(struct.pack("<384I", *seed))
                expected[label] = struct.pack("<384I", *want)
                rows.append({"case": label, "program": tag, "dxil_sha256": digest(blob)})
        for kind, inst in [("negated_source", "v_frexp_mant_f16_e64 v10, -v4"),
                           ("absolute_source", "v_cvt_pknorm_i16_f16 v13, |v4|, v5"),
                           ("clamp", "v_ldexp_f16_e64 v12, v4, v6 clamp"),
                           ("op_sel", "v_cvt_pknorm_u16_f16 v14, v4, v5 op_sel:[1,0,0]"),
                           ("scalar_source", "v_cvt_pknorm_i16_f32 v15, s4, v5"),
                           ("constant_source", "v_frexp_exp_i16_f16_e32 v11, 1.0"),
                           ("sdwa", "v_frexp_mant_f16_sdwa v10, v4 dst_sel:WORD_1 dst_unused:UNUSED_PRESERVE src0_sel:WORD_0")]:
            bad = translate(a.emitter, assemble([inst, "s_endpgm"], a.llvm_mc))
            if bad.returncode != 65:
                raise ValueError(f"unexpected acceptance {inst}")
            negatives.append({"kind": kind, "assembly": inst, "refusal": bad.stderr.strip()})
        (trial / "cases.txt").write_text("".join(f"{n} 4 structured {GROUPS}\n" for n in expected))
        exe, status, fields_ = run_gpu_guest(trial / "guest.cpp", "halfexact.exe", trial, a.work, a.proton,
                                             a.prefix, a.provider, a.compiler_dir)
        if status or fields_.get("adapter") != a.adapter:
            raise RuntimeError(f"guest failed: {status} {fields_}")
        failures = []
        for row in rows:
            actual = trial / (row["case"] + ".bin")
            observed = actual.read_bytes() if actual.is_file() else b""
            if observed != expected[row["case"]]:
                got_words = struct.unpack(f"<{len(observed) // 4}I", observed)
                want_words = struct.unpack("<384I", expected[row["case"]])
                failures.append({"case": row["case"], "different_words": [
                    (i, f"{x:08x}", f"{y:08x}") for i, (x, y) in enumerate(zip(got_words, want_words)) if x != y][:16]})
            else:
                row["readback_sha256"] = digest(actual)
        receipt = {"scope": "Synthetic LLVM-assembled f16 frexp and ldexp and normalized 16-bit pack instructions under eleven MODE values; dynamic instrumented inputs and observed outputs",
                   "adapter": fields_["adapter"], "proton": (a.proton / "version").read_text().strip(),
                   "seed": SEED, "instructions": len({n for n, *_ in FORMS}), "modes": [effective(m) for m in MODES],
                   "shader_cases": len(rows), "inputs": inputs_used,
                   "result_words": inputs_used * OUTPUTS, "compared_words": 384 * len(rows),
                   "mismatches": len(failures), "failures": failures,
                   "baseline_refused_programs": len(variants) if a.baseline else None,
                   "negative_controls": negatives, "tool_sha256": tool_hashes,
                   "guest_sha256": digest(exe), "programs": programs, "rows": rows}
    with a.output.open("x") as f:
        json.dump(receipt, f, indent=2)
        f.write("\n")
    if failures:
        print(f"FAIL {len(failures)} of {len(rows)} dispatches; first: {failures[0]}")
        raise SystemExit(1)
    print(f"PASS {inputs_used} inputs, {receipt['result_words']} results, {receipt['compared_words']} GPU words; zero mismatches")


if __name__ == "__main__":
    main()
