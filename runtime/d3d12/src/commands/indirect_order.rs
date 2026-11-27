//! Whether an `ExecuteIndirect` reads its arguments after the commands that
//! wrote them (brcly/TuxBox#232).
//!
//! A title on the console can write an argument or count buffer and read it
//! a few commands later with only a UAV barrier, or a transition to a
//! combined read state, in between. Two things then go wrong on the host:
//!
//! - A UAV barrier orders shader accesses only. Nothing makes the indirect
//!   read wait for the write, and an NVIDIA driver was measured reading the
//!   previous contents in 48 of 48 tries.
//! - With a count buffer, vkd3d-proton copies a dispatch's records in a
//!   compute pass of its own, and submits that pass ahead of the list until
//!   the list records a barrier that names indirect arguments: a transition
//!   whose `StateAfter` is exactly `INDIRECT_ARGUMENT`, or an enhanced global
//!   or buffer barrier whose `SyncAfter` has `ALL` or `EXECUTE_INDIRECT` and
//!   whose `AccessAfter` is `COMMON` or has `INDIRECT_ARGUMENT`
//!   (`libs/vkd3d/command.c`). Without one the pass reads the values from
//!   before the batch.
//!
//! So each native list tracks both, and `execute_indirect` records one
//! global barrier where the title's own barriers fall short.
use windows::Win32::Graphics::Direct3D12::*;

/// What the open native list has recorded since its last command that can
/// write a buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Order {
    /// A barrier that makes indirect reads wait for earlier writes.
    synced: bool,
    /// A barrier that vkd3d-proton takes as naming indirect arguments.
    named: bool,
}

impl Order {
    /// A dispatch, a copy or another command that can write a buffer was
    /// recorded. Draws are left out: see `Recording::draw`.
    pub(super) fn written(&mut self) {
        *self = Self::default();
    }

    /// A legacy barrier reached the native list.
    pub(super) fn transition(
        &mut self,
        kind: D3D12_RESOURCE_BARRIER_TYPE,
        flags: D3D12_RESOURCE_BARRIER_FLAGS,
        after: D3D12_RESOURCE_STATES,
    ) {
        if kind != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION || flags == D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY {
            return;
        }
        self.synced |= after.0 & D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT.0 != 0;
        self.named |= after == D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT;
    }

    /// An enhanced global or buffer barrier reached the native list.
    pub(super) fn barrier(&mut self, sync_after: D3D12_BARRIER_SYNC, access_after: D3D12_BARRIER_ACCESS) {
        if sync_after.0 & (D3D12_BARRIER_SYNC_ALL.0 | D3D12_BARRIER_SYNC_EXECUTE_INDIRECT.0) != 0
            && (access_after == D3D12_BARRIER_ACCESS_COMMON
                || access_after.0 & D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT.0 != 0)
        {
            self.synced = true;
            self.named = true;
        }
    }

    /// Enhanced barrier groups reached the native list.
    ///
    /// # Safety
    /// Each group points at `NumBarriers` readable barriers of its type.
    pub(super) unsafe fn groups(&mut self, groups: &[D3D12_BARRIER_GROUP]) {
        for group in groups {
            let count = group.NumBarriers as usize;
            unsafe {
                match group.Type {
                    D3D12_BARRIER_TYPE_GLOBAL => {
                        for item in core::slice::from_raw_parts(group.Anonymous.pGlobalBarriers, count) {
                            self.barrier(item.SyncAfter, item.AccessAfter);
                        }
                    }
                    D3D12_BARRIER_TYPE_BUFFER => {
                        for item in core::slice::from_raw_parts(group.Anonymous.pBufferBarriers, count) {
                            self.barrier(item.SyncAfter, item.AccessAfter);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Whether an `ExecuteIndirect` recorded now needs a barrier first.
    /// `patched` is a dispatch with a count buffer, the one form whose
    /// records vkd3d-proton copies in a pass of its own.
    pub(super) fn needs_barrier(self, patched: bool) -> bool {
        !self.synced || (patched && !self.named)
    }
}

/// Every earlier access, then indirect argument reads. vkd3d-proton takes it
/// as naming indirect arguments.
pub(super) const BARRIER: D3D12_GLOBAL_BARRIER = D3D12_GLOBAL_BARRIER {
    SyncBefore: D3D12_BARRIER_SYNC_ALL,
    SyncAfter: D3D12_BARRIER_SYNC_EXECUTE_INDIRECT,
    AccessBefore: D3D12_BARRIER_ACCESS_COMMON,
    AccessAfter: D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT,
};

#[cfg(test)]
#[path = "../tests/commands/indirect_order.rs"]
mod tests;
