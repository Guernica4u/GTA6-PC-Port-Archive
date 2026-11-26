//! Sixteen-lane row permutations and their host participation check.

use super::{Attr, Atom, Bin, Inst, LaneExit, Lower, Operand, Program, Reg, Structured, V, Why, pred};
use crate::gfx10::Valu;

/// Blocks reached by every host lane: a fetch-inactive shuffle cannot
/// recover registers of invocations absent from a divergent host branch.
fn all_lane_blocks(p: &Program, st: &Structured) -> Option<Vec<bool>> {
    let exit_block = st.exits.len();
    let n = exit_block + 1;
    let mut successors = vec![Vec::new(); n];
    let mut reaches_end = vec![false; n];
    reaches_end[exit_block] = true;
    let mut rpo = vec![usize::MAX; exit_block];
    for (index, block) in crate::ir::reverse_postorder(&p.blocks)
        .into_iter()
        .enumerate()
    {
        rpo[block] = index;
    }
    let mut aborts = vec![false; exit_block];
    for (b, exit) in st.exits.iter().enumerate() {
        match *exit {
            LaneExit::Jump(t) => {
                successors[b].push(t);
                if rpo[b] != usize::MAX && rpo[t] <= rpo[b] {
                    // `Lower::exit` adds a per-lane return at the iteration
                    // limit of an unconditional back edge. It bypasses the
                    // loop's normal exit and every later reconvergence point.
                    successors[b].push(exit_block);
                    aborts[b] = true;
                }
            }
            LaneExit::Branch {
                taken, not_taken, ..
            } => {
                successors[b].extend([taken, not_taken]);
            }
            LaneExit::End => successors[b].push(exit_block),
            LaneExit::Indirect => return None,
        }
    }
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            if !reaches_end[b] && successors[b].iter().any(|&q| reaches_end[q]) {
                reaches_end[b] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Post-dominance without any path to the exit would be vacuous.
    if reaches_end.iter().any(|&reaches| !reaches) {
        return None;
    }
    let words = n.div_ceil(64);
    let mut post = vec![vec![u64::MAX; words]; n];
    for (b, next) in successors.iter().enumerate() {
        if next.is_empty() {
            post[b].fill(0);
            post[b][b / 64] |= 1 << (b % 64);
        }
    }
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            if successors[b].is_empty() {
                continue;
            }
            let mut row = vec![u64::MAX; words];
            for &q in &successors[b] {
                for (word, &other) in row.iter_mut().zip(&post[q]) {
                    *word &= other;
                }
            }
            row[b / 64] |= 1 << (b % 64);
            if row != post[b] {
                post[b] = row;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut all = vec![true; n];
    for (branch, exit) in st.exits.iter().enumerate() {
        let divergent = match *exit {
            LaneExit::Branch { when, .. } => !st
                .bools
                .support(when)
                .iter()
                .all(|a| matches!(a, Atom::Uniform(_))),
            _ => false,
        };
        if !divergent && !aborts[branch] {
            continue;
        }
        let mut seen = vec![false; n];
        let mut pending = successors[branch].clone();
        while let Some(b) = pending.pop() {
            if seen[b] || post[branch][b / 64] >> (b % 64) & 1 != 0 {
                continue;
            }
            seen[b] = true;
            all[b] = false;
            pending.extend(successors[b].iter().copied());
        }
    }
    all.truncate(exit_block);
    Some(all)
}

impl Lower<'_> {
    pub(super) fn permlane(&mut self, inst: &Inst, v: &Valu, guard: Option<V>) -> Result<(), Why> {
        let c = self.cur;
        // RDNA2 ISA 12.12: each nibble of {src2, src1} selects a lane
        // in this row of 16, or the other row of the same 32-lane half.
        // OP_SEL[0:1] are fetch-inactive and bound-control, not half selects.
        if v.abs != 0 || v.neg != 0 || v.opsel & !3 != 0 {
            return Err("permlane input modifiers");
        }
        self.cross_lane(false)?;
        let fetch_inactive = v.opsel & 1 != 0;
        if fetch_inactive
            && !all_lane_blocks(self.x.p, self.x.st).is_some_and(|all| all[self.cur_ir])
        {
            // DXIL cannot read the last VGPR value of a host invocation
            // that has left through a lane branch. A guard keeps it here.
            return Err("a permlane fetch from lanes outside the host branch");
        }
        let (x, lo, hi) = (self.src(inst, 0)?, self.src(inst, 1)?, self.src(inst, 2)?);
        let lane = self.lane_index();
        let (k7, k8, k15, k16, k31, k32, k4, one, zero) = (
            self.s.m.i32(7),
            self.s.m.i32(8),
            self.s.m.i32(15),
            self.s.m.i32(16),
            self.s.m.i32(31),
            self.s.m.i32(32),
            self.s.m.i32(4),
            self.s.m.i32(1),
            self.s.m.i32(0),
        );
        let pos = self.s.m.bin(c, Bin::And, lane, k15);
        let lower = self.s.m.cmp(c, pred::IULT, pos, k8);
        let packed = self.s.m.select(c, lower, lo, hi);
        let shift = self.s.m.bin(c, Bin::And, pos, k7);
        let shift = self.s.m.bin(c, Bin::Mul, shift, k4);
        let pick = self.s.m.bin(c, Bin::LShr, packed, shift);
        let pick = self.s.m.bin(c, Bin::And, pick, k15);
        let not15 = self.s.m.i32(!15);
        let mut row = self.s.m.bin(c, Bin::And, lane, not15);
        if inst.name() == "v_permlanex16_b32" {
            row = self.s.m.bin(c, Bin::Xor, row, k16);
        }
        let src = self.s.m.bin(c, Bin::Or, row, pick);
        let r = if fetch_inactive {
            self.read_lane(x, src)
        } else {
            // Ballot the console EXEC guard as well as the host's active
            // invocations. Never shuffle from an inactive host lane.
            let enabled = guard.unwrap_or_else(|| self.s.m.bool(true));
            let i32t = self.s.t.i32;
            let four = self.s.m.named_struct("dx.types.fouri32", vec![i32t; 4]);
            let mask =
                self.s
                    .op(c, "waveActiveBallot", Attr::NoUnwind, four, 116, &[enabled]);
            let lo = self.s.m.extract(c, mask, 0);
            let hi = self.s.m.extract(c, mask, 1);
            let lower = self.s.m.cmp(c, pred::IULT, src, k32);
            let word = self.s.m.select(c, lower, lo, hi);
            let bit = self.s.m.bin(c, Bin::And, src, k31);
            let bit = self.s.m.bin(c, Bin::LShr, word, bit);
            let bit = self.s.m.bin(c, Bin::And, bit, one);
            let active = self.s.m.cmp(c, pred::NE, bit, zero);
            let safe = self.s.m.select(c, active, src, lane);
            let read = self.read_lane(x, safe);
            let fallback = if v.opsel & 2 != 0 {
                zero
            } else {
                let Some(Operand::Vector { reg, .. }) = v.dst else {
                    return Err("a permlane result outside a VGPR");
                };
                self.i32_of(inst.read(Reg::V(reg)).ok_or("a permlane old value")?)?
            };
            self.s.m.select(c, active, read, fallback)
        };
        self.write_i(inst, r, guard)
    }
}
