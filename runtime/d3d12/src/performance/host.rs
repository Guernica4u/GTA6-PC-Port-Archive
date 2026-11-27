//! Processor, memory, video memory and graphics card readings, taken on the
//! sampler's own thread.

use super::gpu::Sensors;
use super::stats::{Host, share};
use windows::Win32::Foundation::{FILETIME, LUID};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
    IDXGIAdapter3, IDXGIFactory4,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes};

/// Processor time so far, in 100 ns units: idle and total over all processors,
/// and this process's own.
#[derive(Clone, Copy)]
struct Times {
    idle: u64,
    system: u64,
    game: u64,
}

pub(super) struct Sampler {
    adapter: Option<IDXGIAdapter3>,
    sensors: Option<Sensors>,
    times: Option<Times>,
    reported: bool,
}

impl Sampler {
    pub(super) fn new(luid: LUID) -> Self {
        // SAFETY: the factory and adapter are owned references.
        let adapter = unsafe {
            CreateDXGIFactory1::<IDXGIFactory4>()
                .and_then(|factory| factory.EnumAdapterByLuid::<IDXGIAdapter3>(luid))
                .ok()
        };
        let sensors = adapter.as_ref()
            .and_then(|adapter| crate::device::host_profile::pci_ids(adapter))
            .and_then(|(vendor, device)| Sensors::open(vendor, device));
        Self { adapter, sensors, times: None, reported: false }
    }

    pub(super) fn read(&mut self) -> Host {
        let mut host = Host::default();
        let times = times();
        if let (Some(before), Some(now)) = (self.times, times) {
            // System kernel time includes the idle time.
            let total = now.system.saturating_sub(before.system);
            let idle = now.idle.saturating_sub(before.idle);
            host.cpu_system = share(total.saturating_sub(idle), total);
            host.cpu_game = share(now.game.saturating_sub(before.game), total);
        }
        self.times = times;
        let mut memory = PROCESS_MEMORY_COUNTERS::default();
        let size = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: the output is a local of the size passed.
        if unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut memory, size) }.is_ok() {
            host.game_resident = Some(memory.WorkingSetSize as u64);
            // Wine leaves the extended structure's private usage at zero.
            host.game_committed = Some(memory.PagefileUsage as u64);
        }
        let mut system = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        // SAFETY: the structure is local and carries its own length.
        if unsafe { GlobalMemoryStatusEx(&mut system) }.is_ok() {
            host.ram = Some((system.ullTotalPhys.saturating_sub(system.ullAvailPhys), system.ullTotalPhys));
            // The page file limits count physical memory too; the rest is swap.
            let total = system.ullTotalPageFile.saturating_sub(system.ullTotalPhys);
            let free = system.ullAvailPageFile.saturating_sub(system.ullAvailPhys);
            host.swap = Some((total.saturating_sub(free), total));
        }
        if let Some(adapter) = &self.adapter {
            let mut video = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            // SAFETY: the output is local.
            if unsafe { adapter.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut video) }.is_ok() {
                host.video = Some((video.CurrentUsage, video.Budget));
            }
        }
        if let Some(sensors) = &self.sensors {
            host.gpu = sensors.read();
            if !std::mem::replace(&mut self.reported, true) {
                crate::diagnostics::log(&format!("first graphics card reading: {:?}", host.gpu));
            }
        }
        host
    }
}

fn times() -> Option<Times> {
    let whole = |time: FILETIME| u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime);
    let [mut idle, mut kernel, mut user] = [FILETIME::default(); 3];
    let [mut created, mut ended, mut own_kernel, mut own_user] = [FILETIME::default(); 4];
    // SAFETY: every output is a local.
    unsafe {
        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()?;
        GetProcessTimes(GetCurrentProcess(), &mut created, &mut ended, &mut own_kernel, &mut own_user).ok()?;
    }
    Some(Times {
        idle: whole(idle),
        system: whole(kernel) + whole(user),
        game: whole(own_kernel) + whole(own_user),
    })
}
