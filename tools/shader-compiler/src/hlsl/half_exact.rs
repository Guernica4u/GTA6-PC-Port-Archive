//! Exact half-precision exponent forms and normalized 16-bit packs:
//! V_FREXP_MANT_F16, V_FREXP_EXP_I16_F16, V_LDEXP_F16, V_CVT_PKNORM_I16_F16,
//! V_CVT_PKNORM_U16_F16 and V_CVT_PKNORM_I16_F32. Integer arithmetic on the
//! stored bits, following MODE: the f16 round field (bits 3:2), f16 denormals
//! (bits 7:6) and IEEE NaN quieting (bit 9). Native RDNA2 execution settled
//! the details (tools/native_rdna2/check_half.py): V_LDEXP_F16 takes a 16-bit
//! signed exponent and tests tininess after rounding, the frexp and ldexp
//! forms keep the high destination half, and the packs ignore MODE except
//! f16 input denormals. Plain VGPR operands, in compute shaders only.
use super::{r, Emitter, Enc, Insn, Opnd};
use crate::Stage;

pub(super) const HELPERS: &str = r#"
uint tb_f16_frexp_mant(uint h, uint mode) {
    h &= 0xffffu;
    uint s = h & 0x8000u, e = (h >> 10) & 31u, m = h & 1023u;
    if (e == 0u && (mode & 0x40u) == 0u) return s;
    if (e == 31u) return m != 0u && (mode & 0x200u) != 0u ? h | 0x200u : h;
    if (e == 0u && m == 0u) return h;
    if (e == 0u) m = (m << (10u - firstbithigh(m))) & 1023u;
    return s | 0x3800u | m;
}
uint tb_f16_frexp_exp(uint h, uint mode) {
    uint e = (h >> 10) & 31u, m = h & 1023u;
    if (e == 31u) return 0u;
    if (e == 0u) return m == 0u || (mode & 0x40u) == 0u ? 0u : uint(int(firstbithigh(m)) - 23) & 0xffffu;
    return uint(int(e) - 14) & 0xffffu;
}
uint tb_f16_ldexp(uint h, uint k, uint mode) {
    h &= 0xffffu;
    uint s = h & 0x8000u, e = (h >> 10) & 31u, m = h & 1023u;
    if (e == 31u) return m != 0u && (mode & 0x200u) != 0u ? h | 0x200u : h;
    if (e == 0u && (mode & 0x40u) == 0u) m = 0u;
    if (e == 0u && m == 0u) return s;
    // Normalized significand S (bit 10 set) with value S * 2^(E - 10).
    uint S = m | 0x400u;
    int E = int(e) - 15;
    if (e == 0u) {
        uint p = firstbithigh(m);
        S = m << (10u - p);
        E = int(p) - 24;
    }
    // The exponent is the low 16 bits of S1, signed. Beyond this range every
    // finite nonzero input overflows or underflows.
    E += clamp(asint(k << 16) >> 16, -64, 64);
    uint rmode = (mode >> 2) & 3u;
    bool neg = s != 0u;
    if (E > 15) return (rmode == 0u || (rmode == 1u && !neg) || (rmode == 2u && neg)) ? s | 0x7c00u : s | 0x7bffu;
    if (E >= -14) return s | (uint(E + 15) << 10) | (S & 1023u);
    // An 11-bit significand rounds to itself, so every such result is tiny.
    if ((mode & 0x80u) == 0u) return s;
    // Denormal units are 2^-24; beyond a 12-bit shift nothing above the midpoint remains.
    uint sh = min(uint(-14 - E), 12u);
    uint kept = S >> sh, rem = S & ((1u << sh) - 1u), half = 1u << (sh - 1u);
    bool up = rmode == 0u ? (rem > half || (rem == half && (kept & 1u) != 0u))
        : rem != 0u && ((rmode == 1u && !neg) || (rmode == 2u && neg));
    return s | (kept + (up ? 1u : 0u));
}
uint tb_norm_f32(uint x, bool signedResult) {
    uint magnitude = x & 0x7fffffffu;
    bool negative = (x & 0x80000000u) != 0u;
    if (magnitude > 0x7f800000u || (negative && !signedResult)) return 0u;
    uint scale = signedResult ? 32767u : 65535u;
    uint result = scale;
    if (magnitude < 0x3f800000u) {
        // Round the exact product sig * scale * 2^-shift once, to nearest even.
        uint e = magnitude >> 23;
        uint sig = (magnitude & 0x7fffffu) | (e == 0u ? 0u : 0x800000u);
        uint shift = 150u - max(e, 1u);
        if (shift > 40u) return 0u;
        uint low = (sig & 0xffffu) * scale, high = (sig >> 16) * scale;
        // The product is high * 2^16 + low; keep it above bit 16 plus a sticky bit.
        uint p = high + (low >> 16);
        bool sticky = (low & 0xffffu) != 0u;
        uint sh = shift - 16u;
        result = p >> sh;
        uint rem = p & ((1u << sh) - 1u), midpoint = 1u << (sh - 1u);
        if (rem > midpoint || (rem == midpoint && (sticky || (result & 1u) != 0u))) ++result;
    }
    return negative ? ((0u - result) & 0xffffu) : result;
}
"#;

pub(super) fn handles(m: &str) -> bool {
    keeps_high_half(m) || packs(m)
}

/// V_CVT_PKNORM_U16_F32, lowered in vector_half.rs, also uses tb_norm_f32.
pub(super) fn needs_helpers(m: &str) -> bool {
    handles(m) || m == "v_cvt_pknorm_u16_f32"
}

/// The frexp and ldexp forms write only the low destination half.
pub(super) fn keeps_high_half(m: &str) -> bool {
    matches!(m, "v_frexp_mant_f16" | "v_frexp_exp_i16_f16" | "v_ldexp_f16")
}

fn packs(m: &str) -> bool {
    matches!(m, "v_cvt_pknorm_i16_f16" | "v_cvt_pknorm_u16_f16" | "v_cvt_pknorm_i16_f32")
}

/// The pack of two f16 values uses the shared normalized-half helper.
pub(super) fn uses_normalized_half(m: &str) -> bool {
    matches!(m, "v_cvt_pknorm_i16_f16" | "v_cvt_pknorm_u16_f16")
}

impl Emitter<'_> {
    pub(in crate::hlsl) fn lower_half_exact(&mut self, ins: &Insn) -> Result<bool, String> {
        let m = ins.mnemonic.as_str();
        if !handles(m) {
            return Ok(false);
        }
        let arity = if matches!(m, "v_frexp_mant_f16" | "v_frexp_exp_i16_f16") { 1 } else { 2 };
        // VOP1/VOP2 without SDWA or DPP, or VOP3 without abs, neg, clamp, omod or op_sel.
        let plain = match ins.enc {
            Enc::Vop1 | Enc::Vop2 => !packs(m) && ins.aux.sdwa.is_none() && ins.raw.len() == 1,
            Enc::Vop3 => ins.raw.len() == 2 && ins.raw[0] & 0xff00 == 0 && ins.raw[1] >> 27 == 0,
            _ => false,
        };
        let Some(d @ Opnd::Vgpr(_)) = ins.dst.first().copied() else {
            return Err("unsupported exact half operand".into());
        };
        if ins.src.len() < arity || !ins.src[..arity].iter().all(|s| matches!(s, Opnd::Vgpr(_))) {
            return Err("unsupported exact half operand".into());
        }
        if !plain || !ins.is_complete() {
            return Err("unsupported exact half encoding or modifiers".into());
        }
        if self.ex.stage != Stage::Compute {
            return Err("unverified exact half shader stage".into());
        }
        let a = r(&ins.src[0])?;
        let mode = if self.regs.contains("tb_fp_mode") { "tb_fp_mode" } else { "0xc0u" };
        let value = match m {
            "v_frexp_mant_f16" => format!("tb_f16_frexp_mant({a}, {mode})"),
            "v_frexp_exp_i16_f16" => format!("tb_f16_frexp_exp({a}, {mode})"),
            "v_ldexp_f16" => format!("tb_f16_ldexp({a}, {}, {mode})", r(&ins.src[1])?),
            "v_cvt_pknorm_i16_f32" => format!("(tb_norm_f32({a}, true) | (tb_norm_f32({}, true) << 16))", r(&ins.src[1])?),
            _ => {
                let signed = m == "v_cvt_pknorm_i16_f16";
                format!("(tb_norm_f16({a}, {mode}, {signed}) | (tb_norm_f16({}, {mode}, {signed}) << 16))", r(&ins.src[1])?)
            }
        };
        if keeps_high_half(m) {
            self.touch(&d);
            return self.wv(&d, &format!("(({} & 0xffff0000u) | {value})", r(&d)?)).map(|()| true);
        }
        self.wv(&d, &value).map(|()| true)
    }
}

#[cfg(test)]
#[path = "../tests/hlsl/half_exact.rs"]
mod tests;
