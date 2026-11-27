//! Byte SADs over four sliding windows: V_QSAD_PK_U16_U8, V_MQSAD_PK_U16_U8
//! and V_MQSAD_U32_U8. S0 is a VGPR pair whose window k is bits [8k+31:8k],
//! S1 the reference word, and S2 the accumulators: four 16-bit fields in a
//! pair, or four words for V_MQSAD_U32_U8. The masked forms skip a byte whose
//! S1 byte is zero, and without clamp the 16-bit fields wrap. Native RDNA2
//! execution settled both (tools/native_rdna2/check_sad.py). Plain VGPR
//! operands without modifiers, in compute shaders only.
use super::{r, Emitter, Enc, Insn, Opnd};
use crate::Stage;

pub(super) fn handles(m: &str) -> bool {
    matches!(m, "v_qsad_pk_u16_u8" | "v_mqsad_pk_u16_u8" | "v_mqsad_u32_u8")
}

/// Extra registers after the first of the S2 accumulator and the result.
fn words(m: &str) -> u32 {
    if m == "v_mqsad_u32_u8" { 3 } else { 1 }
}

pub(super) fn result_extra(m: &str) -> u32 {
    if handles(m) { words(m) } else { 0 }
}

/// High source registers read beyond the decoded operands.
pub(super) fn source_highs(ins: &Insn) -> Vec<u32> {
    if !handles(&ins.mnemonic) {
        return Vec::new();
    }
    let mut highs = Vec::new();
    if let Some(Opnd::Vgpr(a)) = ins.src.first() {
        highs.push(u32::from(*a) + 1);
    }
    if let Some(Opnd::Vgpr(c)) = ins.src.get(2) {
        highs.extend((1..=words(&ins.mnemonic)).map(|k| u32::from(*c) + k));
    }
    highs
}

/// Sum of byte differences between `x` and `y`, skipping zero bytes of `y` when masked.
pub(super) fn sad(x: &str, y: &str, masked: bool) -> String {
    (0..4).map(|i| {
        let p = format!("(({x} >> {}u) & 0xffu)", 8 * i);
        let q = format!("(({y} >> {}u) & 0xffu)", 8 * i);
        let diff = format!("(max({p}, {q}) - min({p}, {q}))");
        if masked { format!("({q} != 0u ? {diff} : 0u)") } else { diff }
    }).collect::<Vec<_>>().join(" + ")
}

impl Emitter<'_> {
    pub(in crate::hlsl) fn lower_quad_sad(&mut self, ins: &Insn) -> Result<bool, String> {
        let m = ins.mnemonic.as_str();
        if !handles(m) {
            return Ok(false);
        }
        let extra = words(m) as u8;
        let (Some(Opnd::Vgpr(dst)), [Opnd::Vgpr(a), Opnd::Vgpr(b), Opnd::Vgpr(c), ..]) = (ins.dst.first(), &ins.src[..]) else {
            return Err("unsupported quad SAD operand".into());
        };
        let (dst, a, b, c) = (*dst, *a, *b, *c);
        // VOP3 without clamp, op_sel or negation.
        if ins.enc != Enc::Vop3 || !ins.is_complete() || ins.raw[0] & 0xff00 != 0 || ins.raw[1] >> 27 != 0 {
            return Err("unsupported quad SAD encoding or modifiers".into());
        }
        if self.ex.stage != Stage::Compute {
            return Err("unverified quad SAD shader stage".into());
        }
        if dst.checked_add(extra).is_none() || a == u8::MAX || c.checked_add(extra).is_none() {
            return Err("VGPR range overflows".into());
        }
        let reg = |n: u8| Opnd::Vgpr(n);
        for n in [a + 1].into_iter().chain((1..=extra).map(|k| c + k)) {
            self.touch(&reg(n));
        }
        let (lo, hi, y) = (r(&reg(a))?, r(&reg(a + 1))?, r(&reg(b))?);
        let t = self.tmp;
        self.tmp += 1;
        let masked = m.starts_with("v_mqsad");
        let mut sums = Vec::new();
        for k in 0..4u8 {
            let window = if k == 0 { lo.clone() } else { format!("(({lo} >> {}u) | ({hi} << {}u))", 8 * k, 32 - 8 * k) };
            self.line(&format!("uint t{t}w{k} = {window};"));
            sums.push(sad(&format!("t{t}w{k}"), &y, masked));
        }
        let mut results = Vec::new();
        if m == "v_mqsad_u32_u8" {
            for (k, sum) in sums.iter().enumerate() {
                results.push(format!("{sum} + {}", r(&reg(c + k as u8))?));
            }
        } else {
            let (acc_lo, acc_hi) = (r(&reg(c))?, r(&reg(c + 1))?);
            let field = |k: usize| {
                let acc = if k < 2 { &acc_lo } else { &acc_hi };
                format!("((({}) + (({acc} >> {}u) & 0xffffu)) & 0xffffu)", sums[k], 16 * (k % 2))
            };
            results.push(format!("{} | ({} << 16u)", field(0), field(1)));
            results.push(format!("{} | ({} << 16u)", field(2), field(3)));
        }
        // Every result word is computed before any destination word is written.
        for (k, value) in results.iter().enumerate() {
            self.line(&format!("uint t{t}r{k} = {value};"));
        }
        for k in 0..results.len() {
            self.wv(&reg(dst + k as u8), &format!("t{t}r{k}"))?;
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tests/hlsl/quad_sad.rs"]
mod tests;
