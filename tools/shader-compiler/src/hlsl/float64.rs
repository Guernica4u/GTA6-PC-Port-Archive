//! Exactly specified double-precision forms on register pairs: rounding to
//! an integral value, conversions to and from f32 and 32-bit integers,
//! minimum and maximum, frexp, fract and ldexp, plus the f32 frexp pair.
//! Like the f64 comparisons they use bit operations on `uint2` (low, high)
//! pairs, so no host FP64 support is required. The results follow MODE:
//! round (bits 3:0), denormals (bits 7:4) and IEEE NaN quieting (bit 9).
use super::{r, Emitter, Enc, Insn, Opnd};

pub(super) const HELPERS: &str = r#"
uint2 tb_f64_round(uint2 x, uint mode, uint fpMode) {
    // mode: 0 toward zero, 1 floor, 2 ceil, 3 nearest even. Only IEEE mode
    // quiets a signaling NaN.
    uint e = (x.y >> 20) & 0x7ffu, sign = x.y & 0x80000000u;
    bool quietNaN = (fpMode & 0x200u) != 0u && ((x.y & 0xfffffu) | x.x) != 0u;
    if (e == 0x7ffu) return quietNaN ? uint2(x.x, x.y | 0x80000u) : x;
    if (e >= 1075u) return x;
    if (e == 0u && (fpMode & 0x40u) == 0u) x = uint2(0u, sign);
    bool nonzero = ((x.y & 0x7fffffffu) | x.x) != 0u;
    if (e < 1023u) {
        bool one = mode == 1u ? (sign != 0u && nonzero) : mode == 2u ? (sign == 0u && nonzero)
            : mode == 3u && e == 1022u && ((x.y & 0xfffffu) | x.x) != 0u;
        return uint2(0u, sign | (one ? 0x3ff00000u : 0u));
    }
    uint f = 1075u - e;
    uint2 mask = f >= 32u ? uint2(0xffffffffu, (1u << (f - 32u)) - 1u) : uint2((1u << f) - 1u, 0u);
    uint2 unit = f >= 32u ? uint2(0u, 1u << (f - 32u)) : uint2(1u << f, 0u);
    uint2 frac = x & mask, t = x & ~mask;
    bool hasFrac = (frac.x | frac.y) != 0u, up = false;
    if (mode == 1u) up = sign != 0u && hasFrac;
    if (mode == 2u) up = sign == 0u && hasFrac;
    if (mode == 3u) {
        uint2 half = f > 32u ? uint2(0u, 1u << (f - 33u)) : uint2(1u << (f - 1u), 0u);
        bool above = frac.y > half.y || (frac.y == half.y && frac.x > half.x);
        bool tie = frac.x == half.x && frac.y == half.y;
        up = above || (tie && ((t.x & unit.x) | (t.y & unit.y)) != 0u);
    }
    if (up) {
        uint lo = t.x + unit.x;
        t = uint2(lo, t.y + unit.y + (lo < t.x ? 1u : 0u));
    }
    return t;
}
uint tb_f64_to_i32(uint2 x, bool signedResult) {
    // Truncate; saturate out-of-range values and return 0 for NaN.
    uint e = (x.y >> 20) & 0x7ffu;
    bool neg = (x.y >> 31) != 0u;
    if (e == 0x7ffu && ((x.y & 0xfffffu) | x.x) != 0u) return 0u;
    if (e < 1023u) return 0u;
    if (signedResult && e >= 1054u) return neg ? 0x80000000u : 0x7fffffffu;
    if (!signedResult && neg) return 0u;
    if (!signedResult && e >= 1055u) return 0xffffffffu;
    uint s = 52u - (e - 1023u);
    uint hi = (x.y & 0xfffffu) | 0x100000u;
    uint v = s >= 32u ? hi >> (s - 32u) : (hi << (32u - s)) | (x.x >> s);
    return signedResult && neg ? 0u - v : v;
}
uint2 tb_i32_to_f64(uint v, bool signedInput) {
    uint sign = 0u;
    if (signedInput && (v >> 31) != 0u) { sign = 0x80000000u; v = 0u - v; }
    if (v == 0u) return uint2(0u, 0u);
    uint p = firstbithigh(v), rest = v ^ (1u << p), sh = 52u - p;
    uint2 m = sh >= 32u ? uint2(0u, rest << (sh - 32u)) : uint2(rest << sh, rest >> (32u - sh));
    return uint2(m.x, sign | ((1023u + p) << 20) | m.y);
}
uint2 tb_u64_shl(uint2 x, uint s) {
    if (s == 0u) return x;
    if (s >= 64u) return uint2(0u, 0u);
    if (s >= 32u) return uint2(0u, x.x << (s - 32u));
    return uint2(x.x << s, (x.y << s) | (x.x >> (32u - s)));
}
uint2 tb_u64_shr(uint2 x, uint s) {
    if (s == 0u) return x;
    if (s >= 64u) return uint2(0u, 0u);
    if (s >= 32u) return uint2(x.y >> (s - 32u), 0u);
    return uint2((x.x >> s) | (x.y << (32u - s)), x.y >> s);
}
uint2 tb_u64_sub(uint2 a, uint2 b) { return uint2(a.x - b.x, a.y - b.y - (a.x < b.x ? 1u : 0u)); }
bool tb_u64_less(uint2 a, uint2 b) { return a.y < b.y || (a.y == b.y && a.x < b.x); }
uint tb_u64_lead(uint2 x) { return x.y != 0u ? 32u + firstbithigh(x.y) : firstbithigh(x.x); }
bool tb_round_up(uint2 x, uint s, uint rmode, bool neg) {
    // Whether dropping the low s bits of the magnitude x rounds it up.
    // rmode: 0 nearest even, 1 toward +inf, 2 toward -inf, 3 toward zero.
    if (s == 0u) return false;
    uint2 lost = s >= 64u ? x : tb_u64_sub(x, tb_u64_shl(tb_u64_shr(x, s), s));
    if ((lost.x | lost.y) == 0u || rmode == 3u) return false;
    if (rmode != 0u) return (rmode == 1u) != neg;
    if (s > 64u) return false;
    uint2 half = tb_u64_shl(uint2(1u, 0u), s - 1u);
    if (tb_u64_less(half, lost)) return true;
    return all(lost == half) && (tb_u64_shr(x, s).x & 1u) != 0u;
}
bool tb_overflow_inf(uint rmode, bool neg) {
    return rmode == 0u || (rmode == 1u && !neg) || (rmode == 2u && neg);
}
bool tb_f64_nan(uint2 x) { return (x.y & 0x7fffffffu) > 0x7ff00000u || ((x.y & 0x7fffffffu) == 0x7ff00000u && x.x != 0u); }
uint2 tb_f64_flush(uint2 x, uint mode) {
    return (x.y & 0x7ff00000u) == 0u && (mode & 0x40u) == 0u ? uint2(0u, x.y & 0x80000000u) : x;
}
uint2 tb_f64_quiet(uint2 x, uint mode) {
    return tb_f64_nan(x) && (mode & 0x200u) != 0u ? uint2(x.x, x.y | 0x80000u) : x;
}
uint2 tb_f32_to_f64(uint a, uint mode) {
    uint sign = a & 0x80000000u, e = (a >> 23) & 255u, m = a & 0x7fffffu;
    if (e == 255u) {
        uint2 r = uint2(m << 29, sign | 0x7ff00000u | (m >> 3));
        return m != 0u && (mode & 0x200u) != 0u ? uint2(r.x, r.y | 0x80000u) : r;
    }
    uint biased = e + 896u;
    if (e == 0u) {
        if (m == 0u || (mode & 0x10u) == 0u) return uint2(0u, sign);
        uint p = firstbithigh(m);
        m = (m << (23u - p)) & 0x7fffffu;
        biased = p + 874u;
    }
    return uint2(m << 29, sign | (biased << 20) | (m >> 3));
}
uint tb_f64_to_f32(uint2 x, uint mode) {
    uint sign = x.y & 0x80000000u, e = (x.y >> 20) & 0x7ffu;
    uint2 m = uint2(x.x, x.y & 0xfffffu);
    bool neg = sign != 0u;
    if (e == 0x7ffu)
        return sign | ((m.x | m.y) != 0u ? 0x7fc00000u | (m.y << 3) | (m.x >> 29) : 0x7f800000u);
    if (e == 0u && ((m.x | m.y) == 0u || (mode & 0x40u) == 0u)) return sign;
    // Significand S with value S * 2^(E - 52); f64 denormals keep E = -1022.
    uint2 S = e == 0u ? m : uint2(m.x, m.y | 0x100000u);
    int E = int(max(e, 1u)) - 1023;
    uint rmode = mode & 3u;
    if (E >= -126) {
        uint kept = tb_u64_shr(S, 29u).x + (tb_round_up(S, 29u, rmode, neg) ? 1u : 0u);
        if (kept >= 0x1000000u) { kept >>= 1; E += 1; }
        if (E > 127) return sign | (tb_overflow_inf(rmode, neg) ? 0x7f800000u : 0x7f7fffffu);
        return sign | (uint(E + 127) << 23) | (kept & 0x7fffffu);
    }
    if ((mode & 0x20u) == 0u) {
        // Tininess after rounding: only a value that rounds up to 2^-126 at
        // full precision with an unbounded exponent survives flushing.
        bool carry = E == -127 && e != 0u && tb_round_up(S, 29u, rmode, neg)
            && tb_u64_shr(S, 29u).x == 0xffffffu;
        return sign | (carry ? 0x00800000u : 0u);
    }
    uint s = uint(29 - 126 - E);
    return sign | (tb_u64_shr(S, s).x + (tb_round_up(S, s, rmode, neg) ? 1u : 0u));
}
uint2 tb_f64_extrema(uint2 a, uint2 b, bool maximum, uint mode) {
    a = tb_f64_flush(a, mode);
    b = tb_f64_flush(b, mode);
    bool na = tb_f64_nan(a), nb = tb_f64_nan(b);
    if ((mode & 0x200u) != 0u) {
        // IEEE mode: a signaling NaN operand gives that NaN, quieted.
        if (na && (a.y & 0x80000u) == 0u) return uint2(a.x, a.y | 0x80000u);
        if (nb && (b.y & 0x80000u) == 0u) return uint2(b.x, b.y | 0x80000u);
    }
    if (na) return nb ? a : b;
    if (nb) return a;
    // Total order on the remaining values, with -0 below +0.
    uint2 ka = (a.y >> 31) != 0u ? ~a : uint2(a.x, a.y | 0x80000000u);
    uint2 kb = (b.y >> 31) != 0u ? ~b : uint2(b.x, b.y | 0x80000000u);
    return (maximum ? tb_u64_less(kb, ka) : tb_u64_less(ka, kb)) ? a : b;
}
uint tb_f64_frexp_exp(uint2 x, uint mode) {
    uint e = (x.y >> 20) & 0x7ffu;
    uint2 m = uint2(x.x, x.y & 0xfffffu);
    if (e == 0x7ffu) return 0u;
    if (e == 0u)
        return (m.x | m.y) == 0u || (mode & 0x40u) == 0u ? 0u : uint(int(tb_u64_lead(m)) - 1073);
    return uint(int(e) - 1022);
}
uint2 tb_f64_frexp_mant(uint2 x, uint mode) {
    x = tb_f64_flush(x, mode);
    uint e = (x.y >> 20) & 0x7ffu;
    uint2 m = uint2(x.x, x.y & 0xfffffu);
    if (e == 0x7ffu || (e == 0u && (m.x | m.y) == 0u)) return tb_f64_quiet(x, mode);
    if (e == 0u) m = tb_u64_shl(m, 52u - tb_u64_lead(m));
    return uint2(m.x, (x.y & 0x80000000u) | 0x3fe00000u | (m.y & 0xfffffu));
}
uint2 tb_f64_from_units(uint2 n) {
    // n * 2^-53 for 0 < n < 2^53, which is exact.
    uint p = tb_u64_lead(n);
    uint2 m = tb_u64_shl(n, 52u - p);
    return uint2(m.x, ((p + 970u) << 20) | (m.y & 0xfffffu));
}
uint2 tb_f64_fract(uint2 x, uint mode) {
    x = tb_f64_flush(x, mode);
    uint e = (x.y >> 20) & 0x7ffu;
    uint2 m = uint2(x.x, x.y & 0xfffffu);
    bool neg = (x.y >> 31) != 0u;
    if (e == 0x7ffu) return (m.x | m.y) == 0u ? uint2(0u, 0xfff80000u) : tb_f64_quiet(x, mode);
    if (e >= 1075u || (e == 0u && (m.x | m.y) == 0u)) return uint2(0u, 0u);
    if (!neg && e < 1023u) return e == 0u && (mode & 0x80u) == 0u ? uint2(0u, 0u) : x;
    uint2 S = e == 0u ? m : uint2(m.x, m.y | 0x100000u);
    uint2 unit53 = uint2(0u, 0x200000u);
    if (e >= 1022u) {
        // The fraction of |x| in units of 2^-53 is exact, and so is 1 - it.
        uint2 f = tb_u64_shl(S, e - 1022u);
        f.y &= 0x1fffffu;
        if ((f.x | f.y) == 0u) return uint2(0u, 0u);
        return tb_f64_from_units(neg ? tb_u64_sub(unit53, f) : f);
    }
    // -0.5 < x < 0: round 1 - |x| to the 2^-53 grid of [0.5, 1).
    uint r = 1022u - max(e, 1u);
    uint2 t = tb_u64_shr(S, r);
    uint2 lost = r >= 64u ? S : tb_u64_sub(S, tb_u64_shl(t, r));
    uint2 n = tb_u64_sub(unit53, t);
    if ((lost.x | lost.y) != 0u) {
        uint rmode = (mode >> 2) & 3u;
        bool up = rmode == 1u;
        if (rmode == 0u) {
            uint2 half = tb_u64_shl(uint2(1u, 0u), r - 1u);
            up = r > 64u || tb_u64_less(lost, half) || (all(lost == half) && (n.x & 1u) == 0u);
        }
        if (!up) n = tb_u64_sub(n, uint2(1u, 0u));
    }
    // RDNA returns the largest double below 1 when the result rounds to 1.
    if (n.y >= 0x200000u) return uint2(0xffffffffu, 0x3fefffffu);
    return uint2(n.x, 0x3fe00000u | (n.y & 0xfffffu));
}
uint2 tb_f64_ldexp(uint2 x, int k, uint mode) {
    x = tb_f64_flush(x, mode);
    uint sign = x.y & 0x80000000u, e = (x.y >> 20) & 0x7ffu;
    uint2 m = uint2(x.x, x.y & 0xfffffu);
    bool neg = sign != 0u;
    if (e == 0x7ffu || (e == 0u && (m.x | m.y) == 0u)) return tb_f64_quiet(x, mode);
    // Normalized significand S (bit 52 set) with value S * 2^(E - 52).
    uint2 S = uint2(m.x, m.y | 0x100000u);
    int E = int(e) - 1023;
    if (e == 0u) {
        uint p = tb_u64_lead(m);
        S = tb_u64_shl(m, 52u - p);
        E = int(p) - 1074;
    }
    // Beyond this range every finite nonzero input overflows or underflows.
    E += clamp(k, -2200, 2200);
    uint rmode = (mode >> 2) & 3u;
    if (E > 1023)
        return tb_overflow_inf(rmode, neg) ? uint2(0u, sign | 0x7ff00000u) : uint2(0xffffffffu, sign | 0x7fefffffu);
    if (E >= -1022) return uint2(S.x, sign | (uint(E + 1023) << 20) | (S.y & 0xfffffu));
    // A 53-bit significand rounds to itself, so every such result is tiny.
    if ((mode & 0x80u) == 0u) return uint2(0u, sign);
    uint s = uint(-1022 - E);
    uint2 kept = tb_u64_shr(S, s);
    if (tb_round_up(S, s, rmode, neg)) kept = uint2(kept.x + 1u, kept.y + (kept.x == 0xffffffffu ? 1u : 0u));
    return uint2(kept.x, sign | kept.y);
}
uint tb_f32_frexp_exp(uint a, uint mode) {
    uint e = (a >> 23) & 255u, m = a & 0x7fffffu;
    if (e == 255u) return 0u;
    if (e == 0u) return m == 0u || (mode & 0x10u) == 0u ? 0u : uint(int(firstbithigh(m)) - 148);
    return uint(int(e) - 126);
}
uint tb_f32_frexp_mant(uint a, uint mode) {
    uint sign = a & 0x80000000u, e = (a >> 23) & 255u, m = a & 0x7fffffu;
    if (e == 0u && (mode & 0x10u) == 0u) return sign;
    if (e == 255u) return m != 0u && (mode & 0x200u) != 0u ? a | 0x400000u : a;
    if (e == 0u && m == 0u) return a;
    if (e == 0u) m = (m << (23u - firstbithigh(m))) & 0x7fffffu;
    return sign | 0x3f000000u | m;
}
"#;

/// Number of leading sources that are f64 register pairs.
fn pair_sources(m: &str) -> usize {
    match m {
        "v_trunc_f64" | "v_floor_f64" | "v_ceil_f64" | "v_rndne_f64" | "v_cvt_i32_f64" | "v_cvt_u32_f64"
        | "v_cvt_f32_f64" | "v_frexp_exp_i32_f64" | "v_frexp_mant_f64" | "v_fract_f64" | "v_ldexp_f64" => 1,
        "v_min_f64" | "v_max_f64" => 2,
        _ => 0,
    }
}

/// Forms whose result is an f64 register pair.
pub(super) fn pair_result(m: &str) -> bool {
    matches!(m, "v_trunc_f64" | "v_floor_f64" | "v_ceil_f64" | "v_rndne_f64" | "v_cvt_f64_i32" | "v_cvt_f64_u32"
        | "v_cvt_f64_f32" | "v_min_f64" | "v_max_f64" | "v_frexp_mant_f64" | "v_fract_f64" | "v_ldexp_f64")
}

/// The f32 forms implemented here, with one 32-bit source.
fn single(m: &str) -> bool {
    matches!(m, "v_frexp_exp_i32_f32" | "v_frexp_mant_f32")
}

pub(super) fn handles(m: &str) -> bool {
    pair_sources(m) > 0 || pair_result(m) || single(m)
}

/// High registers of the pair sources, for the live-in analysis.
pub(super) fn pair_source_highs(ins: &Insn) -> Vec<u32> {
    ins.src.iter().take(pair_sources(&ins.mnemonic)).filter_map(|o| match o {
        Opnd::Vgpr(n) => Some(u32::from(*n) + 1),
        _ => None,
    }).collect()
}

impl Emitter<'_> {
    pub(super) fn lower_f64(&mut self, ins: &Insn) -> Result<bool, String> {
        let m = ins.mnemonic.as_str();
        if !handles(m) {
            return Ok(false);
        }
        // Ordinary VGPR operands only; the VOP3 form without abs, neg,
        // clamp or omod. Verified in compute shaders only.
        let plain = match ins.enc {
            Enc::Vop1 => ins.aux.sdwa.is_none() && ins.raw.len() == 1,
            Enc::Vop3 => ins.raw.len() == 2 && ins.raw[0] & 0xff00 == 0 && ins.raw[1] >> 27 == 0,
            _ => false,
        };
        let arity = if matches!(m, "v_min_f64" | "v_max_f64" | "v_ldexp_f64") { 2 } else { 1 };
        let Some(Opnd::Vgpr(dst)) = ins.dst.first() else {
            return Err("unsupported f64 operand".into());
        };
        let sources: Vec<u8> = ins.src.iter().take(arity).filter_map(|o| match o {
            Opnd::Vgpr(n) => Some(*n),
            _ => None,
        }).collect();
        if sources.len() != arity {
            return Err("unsupported f64 operand".into());
        }
        if !plain || !ins.is_complete() {
            return Err("unsupported f64 encoding or modifiers".into());
        }
        if self.ex.stage != super::Stage::Compute {
            return Err("unverified f64 shader stage".into());
        }
        let dst = *dst;
        let pairs = pair_sources(m);
        if (pair_result(m) && dst == u8::MAX) || sources[..pairs].contains(&u8::MAX) {
            return Err("VGPR pair overflows".into());
        }
        let mut args = Vec::new();
        for (j, &src) in sources.iter().enumerate() {
            if j < pairs {
                let high = Opnd::Vgpr(src + 1);
                self.touch(&high);
                args.push(format!("uint2({}, {})", r(&Opnd::Vgpr(src))?, r(&high)?));
            } else {
                args.push(r(&Opnd::Vgpr(src))?);
            }
        }
        let source = &args[0];
        let mode = if self.regs.contains("tb_fp_mode") { "tb_fp_mode" } else { "0xc0u" };
        let value = match m {
            "v_trunc_f64" => format!("tb_f64_round({source}, 0u, {mode})"),
            "v_floor_f64" => format!("tb_f64_round({source}, 1u, {mode})"),
            "v_ceil_f64" => format!("tb_f64_round({source}, 2u, {mode})"),
            "v_rndne_f64" => format!("tb_f64_round({source}, 3u, {mode})"),
            "v_cvt_f64_i32" => format!("tb_i32_to_f64({source}, true)"),
            "v_cvt_f64_u32" => format!("tb_i32_to_f64({source}, false)"),
            "v_cvt_i32_f64" => format!("tb_f64_to_i32({source}, true)"),
            "v_cvt_u32_f64" => format!("tb_f64_to_i32({source}, false)"),
            "v_cvt_f64_f32" => format!("tb_f32_to_f64({source}, {mode})"),
            "v_cvt_f32_f64" => format!("tb_f64_to_f32({source}, {mode})"),
            "v_min_f64" | "v_max_f64" => format!("tb_f64_extrema({source}, {}, {}, {mode})", args[1], m == "v_max_f64"),
            "v_frexp_exp_i32_f64" => format!("tb_f64_frexp_exp({source}, {mode})"),
            "v_frexp_mant_f64" => format!("tb_f64_frexp_mant({source}, {mode})"),
            "v_fract_f64" => format!("tb_f64_fract({source}, {mode})"),
            "v_ldexp_f64" => format!("tb_f64_ldexp({source}, asint({}), {mode})", args[1]),
            "v_frexp_exp_i32_f32" => format!("tb_f32_frexp_exp({source}, {mode})"),
            _ => format!("tb_f32_frexp_mant({source}, {mode})"),
        };
        if !pair_result(m) {
            self.wv(&Opnd::Vgpr(dst), &value)?;
            return Ok(true);
        }
        let t = self.tmp;
        self.tmp += 1;
        self.line(&format!("uint2 t{t}f64 = {value};"));
        self.wv(&Opnd::Vgpr(dst), &format!("t{t}f64.x"))?;
        let high = Opnd::Vgpr(dst + 1);
        self.touch(&high);
        self.wv(&high, &format!("t{t}f64.y"))?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/hlsl/float64.rs"]
pub(super) mod tests;
