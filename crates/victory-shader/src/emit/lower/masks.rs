//! Lane masks: a scalar op that writes one, and a mask read as data.
//!
//! (A child module of `lower` only because that file has a size limit in the
//! repository's asset policy; it is part of the same `Lower`.)

use crate::emit::bitcode::Attr;
use crate::emit::shader::flags;
use crate::ir::{Inst, Value};

use super::{LANE_MASK, Lower, Why};

impl<'a> Lower<'a> {
    /// A scalar op whose destination is a lane mask (`narrow`: 32-bit): the
    /// structure already has the mask the op leaves behind, as a boolean of
    /// its lanes, so lower that instead of refusing the op. Refusing it
    /// marked the destination "a lane mask read as data", and every program
    /// that copies exec before its first branch was refused at that branch -
    /// the prologue's character skinning (`CSComputeSkinningPedBlendShapes*`)
    /// among them, dropped four times a frame in a scene that then stands
    /// still.
    pub(super) fn mask_scalar(
        &mut self,
        inst: &Inst,
        lo: Option<Value>,
        narrow: bool,
    ) -> Result<(), Why> {
        if !narrow {
            return Err(LANE_MASK); // a 64-bit mask: see `pending_masks`
        }
        let at = self.cur;
        let mut first = None;
        let lenient = std::mem::replace(&mut self.lenient, true);
        for &(_, w) in &inst.writes {
            if let Some(&b) = self.x.st.masks.get(&w) {
                let bit = match self.boolean(b, at) {
                    Ok(bit) => bit,
                    Err(_) => {
                        self.lenient = lenient;
                        return Err(LANE_MASK);
                    }
                };
                self.lane_bits.insert(w, bit);
                first.get_or_insert(bit);
            }
        }
        self.lenient = lenient;
        let Some(bit) = first.or_else(|| lo.and_then(|v| self.lane_bits.get(&v).copied())) else {
            return Err(LANE_MASK);
        };
        // A write the structure does not name (the new exec of an
        // `s_and_saveexec`, a reused register) carries the same mask.
        for &(_, w) in &inst.writes {
            if !self.x.st.masks.contains_key(&w) {
                self.lane_bits.insert(w, bit);
            }
        }
        Ok(())
    }

    /// A lane mask's low dword from the bit the emitter keeps for it
    /// (`Lower::lane_bits`): its ballot, taken where it is written.
    pub(super) fn bit_masked(&mut self, v: Value) -> Result<super::V, Why> {
        let bit = *self.lane_bits.get(&v).ok_or(LANE_MASK)?;
        if let Some(&(x, _)) = self.ballots.get(&v) {
            return Ok(x);
        }
        let at = match self.def_block.get(&v) {
            None => self.s.entry,
            Some(&b) if b == self.cur_ir => self.cur,
            Some(&b) => self.tail[b].ok_or("a lane mask written after its use")?,
        };
        let i32t = self.s.t.i32;
        let four = self.s.m.named_struct("dx.types.fouri32", vec![i32t; 4]);
        let r = self.s.op(at, "waveActiveBallot", Attr::NoUnwind, four, 116, &[bit]);
        self.s.flags |= flags::WAVE_OPS;
        let xy = (self.s.m.extract(at, r, 0), self.s.m.extract(at, r, 1));
        self.ballots.insert(v, xy);
        Ok(xy.0)
    }
}
