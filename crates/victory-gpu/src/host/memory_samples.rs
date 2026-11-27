//! Optional accounting for explicit residency operations and DXGI usage.

use std::sync::{Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::{ID3D12Device, ID3D12Resource};

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    TextureHeap,
    SampledTexture,
    WritableTexture,
    DepthTwin,
    SharedBuffer,
    MappedLocalBuffer,
    LocalUavBuffer,
}

impl Kind {
    const ALL: [(Self, &'static str); 7] = [
        (Self::TextureHeap, "texture_heap"),
        (Self::SampledTexture, "sampled_texture"),
        (Self::WritableTexture, "writable_texture"),
        (Self::DepthTwin, "depth_twin"),
        (Self::SharedBuffer, "shared_buffer"),
        (Self::MappedLocalBuffer, "mapped_local_buffer"),
        (Self::LocalUavBuffer, "local_uav_buffer"),
    ];
}

#[derive(Clone, Copy)]
// Size of the allocation handled by each attempt. Mapping and residency
// can both report the same allocation. DXGI reports the process totals.
pub(crate) struct Allocation(pub Kind, pub u64);

impl Allocation {
    pub(crate) fn texture(kind: Kind, device: &ID3D12Device, texture: &ID3D12Resource) -> Self {
        let mut bytes = 0;
        if enabled() {
            // SAFETY: plain queries on the allocated resource and its device.
            let size = unsafe { device.GetResourceAllocationInfo(0, &[texture.GetDesc()]) }.SizeInBytes;
            if size != u64::MAX {
                bytes = size;
            }
        }
        Self(kind, bytes)
    }
}

#[derive(Clone, Copy)]
pub(super) enum Step {
    MakeResident,
    WaitForMapping,
}

#[derive(Clone, Copy)]
struct Counts {
    calls: u64,
    errors: u64,
    bytes: u64,
    elapsed_us: u64,
    max_us: u64,
}

impl Counts {
    const EMPTY: Self = Self { calls: 0, errors: 0, bytes: 0, elapsed_us: 0, max_us: 0 };
}

const EMPTY: [[Counts; 2]; Kind::ALL.len()] = [[Counts::EMPTY; 2]; Kind::ALL.len()];
static COUNTS: Mutex<[[Counts; 2]; Kind::ALL.len()]> = Mutex::new(EMPTY);

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| crate::switch::on("VICTORY_MEMORY_SAMPLES"))
}

pub(super) fn record(Allocation(kind, bytes): Allocation, step: Step, elapsed_us: u64, error: bool) {
    if !enabled() {
        return;
    }
    let mut all = COUNTS.lock().unwrap_or_else(|e| e.into_inner());
    let count = &mut all[kind as usize][step as usize];
    count.calls += 1;
    count.errors += u64::from(error);
    count.bytes = count.bytes.saturating_add(bytes);
    count.elapsed_us += elapsed_us;
    count.max_us = count.max_us.max(elapsed_us);
}

pub(super) fn report(memory: Option<[u64; 4]>) {
    if !enabled() {
        return;
    }
    let counts = std::mem::replace(&mut *COUNTS.lock().unwrap_or_else(|e| e.into_inner()), EMPTY);
    let frame = crate::frame::current();
    if let Some([local_budget, local_used, nonlocal_budget, nonlocal_used]) = memory {
        tracing::info!(
            target: "victory_gpu::memory",
            frame,
            local_budget_mib = local_budget >> 20,
            local_used_mib = local_used >> 20,
            nonlocal_budget_mib = nonlocal_budget >> 20,
            nonlocal_used_mib = nonlocal_used >> 20,
            "DXGI memory sample"
        );
    }
    for (kind, domain) in Kind::ALL {
        let [resident, mapped] = counts[kind as usize];
        if resident.calls + mapped.calls == 0 {
            continue;
        }
        tracing::info!(
            target: "victory_gpu::memory",
            frame,
            domain,
            resident_calls = resident.calls,
            resident_errors = resident.errors,
            resident_bytes = resident.bytes,
            resident_us = resident.elapsed_us,
            resident_max_us = resident.max_us,
            mapped_calls = mapped.calls,
            mapped_errors = mapped.errors,
            mapped_bytes = mapped.bytes,
            mapped_us = mapped.elapsed_us,
            mapped_max_us = mapped.max_us,
            "GPU residency sample"
        );
    }
}
