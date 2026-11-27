//! Lane masks of wave32 pixel and vertex programs on hosts that run 64
//! lanes per wave (see `MaskData::Host`).

use super::{Lower, MaskData};
use crate::emit::bitcode::{Attr, Bin, V, pred};
use crate::emit::shader::flags;

impl Lower<'_> {
    /// For a wave32 program with host lanes (pixel and vertex), whether
    /// this lane is in the upper half of a 64-lane host wave: such a host
    /// runs two console waves' worth of lanes in one, so a 32-bit lane mask
    /// is the ballot's dword of the lane's half, and lane numbers from masks
    /// (`s_ff1`, `v_mbcnt`) count within the half. Never true on a host
    /// running 32 lanes. `None` where masks are not split so.
    pub(super) fn host_half(&mut self) -> Option<V> {
        if self.mask_data != MaskData::Host || self.x.native.wave64 {
            return None;
        }
        self.s.flags |= flags::WAVE_OPS;
        let lane = self.s.op(
            self.cur,
            "waveGetLaneIndex",
            Attr::ReadOnly,
            self.s.t.i32,
            111,
            &[],
        );
        let k32 = self.s.m.i32(32);
        Some(self.s.m.cmp(self.cur, pred::IUGE, lane, k32))
    }

    /// The host lane of `lane` counted within this lane's half (see
    /// [`Lower::host_half`]): `lane` itself where masks are not split.
    pub(super) fn host_lane(&mut self, lane: V) -> V {
        let Some(upper) = self.host_half() else {
            return lane;
        };
        let (k32, z) = (self.s.m.i32(32), self.s.m.i32(0));
        let base = self.s.m.select(self.cur, upper, k32, z);
        self.s.m.bin(self.cur, Bin::Add, lane, base)
    }
}
