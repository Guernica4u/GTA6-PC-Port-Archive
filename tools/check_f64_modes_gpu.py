#!/usr/bin/env python3
"""LLVM-assembled f64 conversion, extrema, frexp, fract and ldexp controls on a D3D12 GPU.

v_cvt_f64_f32, v_cvt_f32_f64, v_min/max_f64, v_frexp_exp_i32/mant_f64,
v_fract_f64, v_ldexp_f64, v_frexp_exp_i32/mant_f32, and v_floor/rndne_f64
for IEEE-mode NaN quieting, under eleven MODE values written with
s_setreg. The reference is an exact rational model of the round, denormal
and IEEE fields; tools/native_rdna2/check_model.py compares the same
model with these instructions executed natively on an RDNA2 GPU.
Every destination holds a stale value beforehand, and one form aliases
its source pair.
"""
import argparse
from fractions import Fraction
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

# v4:v5 and v6:v7 are f64 operands, v8 the ldexp exponent, v9 an f32 operand.
INIT = (0x13572468, 0x24681357, 0x35792468, 0x46813579, 0x57913579, 0x68024680)
# (instruction, destination register, destination words, sources)
FORMS = [("v_cvt_f64_f32", 10, 2, "v9"), ("v_cvt_f32_f64", 12, 1, "v[4:5]"),
         ("v_min_f64", 13, 2, "v[4:5], v[6:7]"), ("v_max_f64", 15, 2, "v[4:5], v[6:7]"),
         ("v_frexp_exp_i32_f64", 17, 1, "v[4:5]"), ("v_frexp_mant_f64", 18, 2, "v[4:5]"),
         ("v_fract_f64", 20, 2, "v[4:5]"), ("v_ldexp_f64", 22, 2, "v[4:5], v8"),
         ("v_frexp_exp_i32_f32", 24, 1, "v9"), ("v_frexp_mant_f32", 25, 1, "v9"),
         ("v_floor_f64", 26, 2, "v[4:5]"), ("v_rndne_f64", 28, 2, "v[4:5]")]
VOP3_ONLY = ("v_min_f64", "v_max_f64", "v_ldexp_f64")
OUTPUTS = 22  # v10..v31, including the aliased v_fract_f64 in v[30:31]
# The guest's guarded view covers words 96..288: six groups of 28 words.
GROUPS, STRIDE, START = 6, 28, 96
# MODE[9:0]: f32 round [1:0], f64 round [3:2], f32 denormals [5:4],
# f64 denormals [7:6], DX10 clamp [8], IEEE [9]. None keeps the default 0xc0.
MODES = [None, 0x000, 0x005, 0x04a, 0x0f5, 0x0fa, 0x0ff, 0x2b0, 0x2c0, 0x2cf, 0x3ff]
SEED = 0x46364d44

Q = Fraction


# ----- Exact reference model -----

def fields(x, w):
    eb, mb = (11, 52) if w == 64 else (8, 23)
    return x >> (w - 1), x >> mb & ((1 << eb) - 1), x & ((1 << mb) - 1), eb, mb


def is_nan(x, w):
    _, e, m, eb, _ = fields(x, w)
    return e == (1 << eb) - 1 and m != 0


def quiet(x, w):
    return x | 1 << (51 if w == 64 else 22)


def flush_in(x, w, mode):
    """Denormal inputs become signed zero unless input denormals are enabled."""
    _, e, m, _, _ = fields(x, w)
    if e == 0 and m and not mode & (0x40 if w == 64 else 0x10):
        return x & 1 << (w - 1)
    return x


def value(x, w):
    s, e, m, eb, mb = fields(x, w)
    bias = (1 << (eb - 1)) - 1
    v = Q(m, 1 << mb) * Q(2) ** (1 - bias) if e == 0 else (1 + Q(m, 1 << mb)) * Q(2) ** (e - bias)
    return -v if s else v


def exponent(a):
    """e with 2^e <= a < 2^(e+1) for a positive rational a."""
    e = a.numerator.bit_length() - a.denominator.bit_length()
    return e - 1 if Q(2) ** e > a else e


def encode(v, sign, w, rmode, keep_out):
    """Round an exact rational to binary64/32. rmode: 0 nearest even, 1 +inf, 2 -inf, 3 zero.

    Output flushing tests tininess after rounding, as IEEE 754 defines it:
    the value rounded to full precision with an unbounded exponent."""
    eb, mb = (11, 52) if w == 64 else (8, 23)
    bias, emax = (1 << (eb - 1)) - 1, (1 << eb) - 2
    sbit = 1 << (w - 1) if sign else 0
    a = abs(v)
    if a == 0:
        return sbit
    e = exponent(a)
    emin = 1 - bias
    toward_inf = (rmode == 1 and not sign) or (rmode == 2 and sign)

    def rounded(q):
        n = a / Q(2) ** q
        lo = n.numerator // n.denominator
        rem = n - lo
        up = rem != 0 and (toward_inf if rmode else rem > Q(1, 2) or (rem == Q(1, 2) and lo & 1))
        return lo + up

    if not keep_out and e < emin and rounded(e - mb) * Q(2) ** (e - mb) < Q(2) ** emin:
        return sbit
    q = max(e, emin) - mb
    n = rounded(q)
    if n >= 1 << (mb + 1):
        n, q = n >> 1, q + 1
    if n < 1 << mb:
        return sbit | n if keep_out else sbit
    biased = q + mb + bias
    if biased > emax:
        inf = rmode == 0 or toward_inf
        return sbit | (((1 << eb) - 1) << mb if inf else emax << mb | ((1 << mb) - 1))
    return sbit | biased << mb | (n - (1 << mb))


def signed32(x):
    return x - (1 << 32) if x >> 31 else x


def reference(name, a, b, c, f, mode):
    """Result bits of `name` for f64 operands a, b, exponent c and f32 operand f under MODE."""
    ieee = mode & 0x200
    if name == "v_cvt_f64_f32":
        if is_nan(f, 32):
            s, _, m, _, _ = fields(f, 32)
            r = s << 63 | 0x7ff << 52 | m << 29
            return quiet(r, 64) if ieee else r
        f = flush_in(f, 32, mode)
        s, e, _, _, _ = fields(f, 32)
        return s << 63 | 0x7ff << 52 if e == 255 else encode(value(f, 32), s, 64, 0, True)
    if name == "v_cvt_f32_f64":
        if is_nan(a, 64):
            s, _, m, _, _ = fields(a, 64)
            return quiet(s << 31 | 0xff << 23 | m >> 29, 32)
        a = flush_in(a, 64, mode)
        s, e, _, _, _ = fields(a, 64)
        return s << 31 | 0xff << 23 if e == 0x7ff else encode(value(a, 64), s, 32, mode & 3, mode & 0x20)
    if name in ("v_min_f64", "v_max_f64"):
        a, b = flush_in(a, 64, mode), flush_in(b, 64, mode)
        na, nb = is_nan(a, 64), is_nan(b, 64)
        if ieee and na and not a >> 51 & 1:
            return quiet(a, 64)
        if ieee and nb and not b >> 51 & 1:
            return quiet(b, 64)
        if na:
            return a if nb else b
        if nb:
            return a
        key = lambda x: (~x & (1 << 64) - 1) if x >> 63 else x | 1 << 63
        pick_a = key(a) > key(b) if name == "v_max_f64" else key(a) < key(b)
        return a if pick_a else b
    if name in ("v_frexp_exp_i32_f64", "v_frexp_exp_i32_f32"):
        w = 64 if name.endswith("f64") else 32
        x = flush_in(a if w == 64 else f, w, mode)
        _, e, _, eb, _ = fields(x, w)
        if e == (1 << eb) - 1 or value(x, w) == 0:
            return 0
        return (exponent(abs(value(x, w))) + 1) % 2**32
    if name in ("v_frexp_mant_f64", "v_frexp_mant_f32"):
        w = 64 if name.endswith("f64") else 32
        x = flush_in(a if w == 64 else f, w, mode)
        s, e, _, eb, _ = fields(x, w)
        if is_nan(x, w):
            return quiet(x, w) if ieee else x
        if e == (1 << eb) - 1 or value(x, w) == 0:
            return x
        v = abs(value(x, w))
        return encode(v / Q(2) ** (exponent(v) + 1), s, w, 0, True)
    if name == "v_ldexp_f64":
        x = flush_in(a, 64, mode)
        s, e, _, _, _ = fields(x, 64)
        if is_nan(x, 64):
            return quiet(x, 64) if ieee else x
        if e == 0x7ff or value(x, 64) == 0:
            return x
        k = max(-2200, min(2200, signed32(c)))
        return encode(value(x, 64) * Q(2) ** k, s, 64, mode >> 2 & 3, mode & 0x80)
    x = flush_in(a, 64, mode)
    s, e, _, _, _ = fields(x, 64)
    if is_nan(x, 64):
        return quiet(x, 64) if ieee else x
    if name == "v_fract_f64":
        if e == 0x7ff:
            return 0xfff8_0000_0000_0000
        v = value(x, 64)
        # RDNA returns the largest double below 1 when the result rounds to 1.
        return min(encode(v - math.floor(v), False, 64, mode >> 2 & 3, mode & 0x80), 0x3fef_ffff_ffff_ffff)
    if e == 0x7ff:
        return x
    v = value(x, 64)
    r = {"v_trunc_f64": math.trunc, "v_floor_f64": math.floor, "v_ceil_f64": math.ceil, "v_rndne_f64": round}[name](v)
    return s << 63 if r == 0 else encode(Q(r), r < 0, 64, 0, True)


def effective(mode):
    return 0xc0 if mode is None else mode


# ----- Inputs -----

def d(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def fb(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


T = 2.0 ** -126
EDGE_F64 = [0, 1 << 63, d(1.0), d(-1.0), d(0.5), d(-2.5), d(1 / 3), d(-1 / 3), d(-1e-17), d(-1e-16),
            0x7ff0_0000_0000_0000, 0xfff0_0000_0000_0000, 0x7ff8_0000_0000_0000, 0xfff8_0000_0000_0000,
            0x7ff0_0000_0000_0001, 0x7ff4_0000_0000_0000, 0xfff0_0000_0000_0123, 0x7ff8_0000_0001_2345,
            1, 0x000f_ffff_ffff_ffff, 0x8000_0000_0000_0001, 0x800f_ffff_ffff_ffff, 0x0010_0000_0000_0000,
            0x7fef_ffff_ffff_ffff, 0xffef_ffff_ffff_ffff, d(1e-300), d(-1e-300),
            d(T - 2.0 ** -151), d(T - 2.0 ** -150), d(T - 2.0 ** -149), d(-(T - 2.0 ** -150)), d(2.0 ** -150),
            d(2.0 ** -150 + 2.0 ** -170), d(3 * 2.0 ** -151), d(2.0 ** 128 - 2.0 ** 103), d(2.0 ** 128 - 2.0 ** 104),
            d(2.0 ** -1022 - 2.0 ** -1075), 0x432f_ffff_ffff_ffff, 0x3caf_ffff_ffff_ffff, 0x3fef_ffff_ffff_ffff,
            0xbc90_0000_0000_0000, 0xbca0_0000_0000_0000, 0xbc8f_ffff_ffff_ffff, d(2.0 ** 52), d(-(2.0 ** 52) - 0.5)]
EDGE_F32 = [0, 0x8000_0000, 1, 0x8000_0001, 0x007f_ffff, 0x807f_ffff, 0x0080_0000, 0x7f7f_ffff, 0x7f80_0000,
            0xff80_0000, 0x7fc0_0000, 0xffc0_0000, 0x7f80_0001, 0x7fa0_0000, 0xff81_2345, fb(1.0), fb(-1.5), fb(0.1)]
EDGE_EXP = [0, 1, -1, 1023, -1022, -1074, -1075, 1024, 2046, -2098, 2**31 - 1, -2**31, 52, -52]


def random_f64(rng):
    k = rng.random()
    if k < 0.15:
        e = rng.choice([0, 0x7ff])
    elif k < 0.45:
        e = rng.choice([1, 2, 0x7fe, 1023, 1022, 1021, 1024, 1075, 1074, 1052, 1053, 1054,
                        896, 897, 874, 873, 872, 1150, 1151, 1152])
    else:
        e = rng.getrandbits(11)
    m = rng.getrandbits(52) if rng.random() < 0.7 else rng.choice(
        [0, 1, 1 << 51, (1 << 52) - 1, 1 << 28, (1 << 29) - 1, 1 << 29 | 1 << 28])
    return rng.getrandbits(1) << 63 | e << 52 | m


def random_f32(rng):
    e = rng.choice([0, 255, 1, 254, 127]) if rng.random() < 0.3 else rng.getrandbits(8)
    return rng.getrandbits(1) << 31 | e << 23 | rng.getrandbits(23)


def inputs(rng, index):
    """(a, b, exponent, f32) for one lane; the first lanes walk the edge lists."""
    if index < len(EDGE_F64) ** 2 and index < 600:
        a, b = EDGE_F64[index % len(EDGE_F64)], EDGE_F64[(index * 7 + index // len(EDGE_F64)) % len(EDGE_F64)]
    else:
        a = random_f64(rng)
        b = random_f64(rng) if rng.random() < 0.7 else rng.choice(EDGE_F64)
    if rng.random() < 0.1:
        b = a ^ rng.getrandbits(1) << 63
    f = EDGE_F32[index] if index < len(EDGE_F32) else random_f32(rng)
    c = (EDGE_EXP[index % len(EDGE_EXP)] if rng.random() < 0.3 else rng.choice(
        [rng.randint(-2200, 2200), rng.getrandbits(32) - 2**31])) % 2**32
    return a, b, c, f


# ----- Program -----

def program(mode, e64):
    lines = [f"v_mov_b32_e32 v{4 + j}, 0x{v:x}" for j, v in enumerate(INIT)]
    if mode is not None:
        lines.append(f"s_setreg_imm32_b32 hwreg(HW_REG_MODE, 0, 10), 0x{mode:x}")
    # Stale destination values; the translator must replace every word.
    lines += [f"v_mov_b32_e32 v{reg}, 0x{0xdead0000 + reg:x}" for reg in range(10, 10 + OUTPUTS)]
    for name, dst, width, sources in FORMS:
        target = f"v[{dst}:{dst + 1}]" if width == 2 else f"v{dst}"
        suffix = "" if name in VOP3_ONLY else "_e64" if e64 else "_e32"
        lines.append(f"{name}{suffix} {target}, {sources}")
    lines += ["v_mov_b32_e32 v30, v4", "v_mov_b32_e32 v31, v5",
              f"v_fract_f64{'_e64' if e64 else '_e32'} v[30:31], v[30:31]", "s_endpgm"]
    return lines


def expected_outputs(a, b, c, f, mode):
    out = []
    for name, _, width, _ in FORMS:
        r = reference(name, a, b, c, f, mode)
        out += [r & 0xffffffff, r >> 32] if width == 2 else [r]
    r = reference("v_fract_f64", a, b, c, f, mode)
    return out + [r & 0xffffffff, r >> 32]


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
    ap.add_argument("--dispatches", type=int, default=24, help="dispatches per variant, six inputs each")
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
                if before.returncode != 65 or "_f" not in before.stderr:
                    raise ValueError(f"baseline did not refuse: {before.returncode} {before.stderr}")
            got = translate(a.emitter, words)
            got.check_returncode()
            source = instrument(got.stdout)
            tag = f"{'default' if mode is None else f'mode{mode:03x}'}{'_e64' if e64 else ''}"
            programs.append({"program": tag, "mode": effective(mode), "mode_written": mode is not None, "e64": e64,
                             "native_words": [f"{v:08x}" for v in words],
                             "emitted_sha256": hashlib.sha256(got.stdout.encode()).hexdigest()})
            for k in range(a.dispatches):
                label = f"f64modes_{tag}_{k:02}"
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
                    seed[offset:offset + len(INIT)] = [x & 0xffffffff, x >> 32, y & 0xffffffff, y >> 32, c, f]
                    want[offset:offset + len(INIT)] = seed[offset:offset + len(INIT)]
                    want[offset + len(INIT):offset + STRIDE] = expected_outputs(x, y, c, f, effective(mode))
                    inputs_used += 1
                (trial / (label + ".seed")).write_bytes(struct.pack("<384I", *seed))
                expected[label] = struct.pack("<384I", *want)
                rows.append({"case": label, "program": tag, "dxil_sha256": digest(blob)})
        for kind, inst in [("negated_source", "v_cvt_f32_f64_e64 v12, -v[4:5]"),
                           ("absolute_source", "v_min_f64 v[13:14], |v[4:5]|, v[6:7]"),
                           ("clamp", "v_fract_f64_e64 v[20:21], v[4:5] clamp"),
                           ("output_modifier", "v_ldexp_f64 v[22:23], v[4:5], v8 mul:2"),
                           ("scalar_source", "v_max_f64 v[15:16], s[4:5], v[6:7]"),
                           ("scalar_exponent", "v_ldexp_f64 v[22:23], v[4:5], s8"),
                           ("constant_source", "v_frexp_mant_f32_e32 v25, 1.0")]:
            bad = translate(a.emitter, assemble([inst, "s_endpgm"], a.llvm_mc))
            if bad.returncode != 65:
                raise ValueError(f"unexpected acceptance {inst}")
            negatives.append({"kind": kind, "assembly": inst, "refusal": bad.stderr.strip()})
        (trial / "cases.txt").write_text("".join(f"{n} 4 structured {GROUPS}\n" for n in expected))
        exe, status, fields_ = run_gpu_guest(trial / "guest.cpp", "f64modes.exe", trial, a.work, a.proton,
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
        receipt = {"scope": "Synthetic LLVM-assembled f64 and f32 conversion, extrema, frexp, fract and ldexp instructions under eleven MODE values; dynamic instrumented inputs and observed outputs",
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
