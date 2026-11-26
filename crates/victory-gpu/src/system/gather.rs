//! Asking this machine for its facts.
//!
//! Nothing here needs a title, a game file or a window: the adapters are
//! enumerated and a device made on each, exactly as the host side makes
//! one (`host`), so what a report says is what a run would meet.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
use windows::Win32::Graphics::Direct3D12::{
    D3D_SHADER_MODEL_6_9, D3D12_DESCRIPTOR_HEAP_DESC, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
    D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, D3D12_FEATURE_D3D12_OPTIONS,
    D3D12_FEATURE_D3D12_OPTIONS1, D3D12_FEATURE_D3D12_OPTIONS7, D3D12_FEATURE_D3D12_OPTIONS16,
    D3D12_FEATURE_DATA_D3D12_OPTIONS, D3D12_FEATURE_DATA_D3D12_OPTIONS1,
    D3D12_FEATURE_DATA_D3D12_OPTIONS7, D3D12_FEATURE_DATA_D3D12_OPTIONS16,
    D3D12_FEATURE_DATA_SHADER_MODEL, D3D12_FEATURE_SHADER_MODEL,
    D3D12_MESH_SHADER_TIER_NOT_SUPPORTED, D3D12CreateDevice, ID3D12DescriptorHeap, ID3D12Device,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIAdapter1, IDXGIDevice, IDXGIFactory1,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_SZ, REG_VALUE_TYPE, RegCloseKey,
    RegOpenKeyExW, RegQueryValueExW,
};
use windows::Win32::System::SystemInformation::{
    GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};
use windows::Win32::System::Threading::{
    IsProcessorFeaturePresent, PF_AVX2_INSTRUCTIONS_AVAILABLE,
};
use windows::core::{IUnknown, Interface, PCWSTR, s, w};

use super::{Cpu, Facts, Gpu, Memory};

/// Views the largest host view heap is tried at, past the title's own (see
/// `RAW_TWINS` in `descriptor/host.rs`).
const MOST_VIEWS: u32 = 1 << 22;

/// Views the next largest is tried at.
const MORE_VIEWS: u32 = 1 << 21;

/// Views a host that takes neither holds: the title's own heap just fits.
const TITLE_VIEWS: u32 = 1_000_000;

pub(crate) fn gather() -> Facts {
    let (gpus, used) = adapters();
    Facts {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        commit: None,
        debug: cfg!(debug_assertions),
        windows: windows_version(),
        cpu: cpu(),
        memory: memory(),
        gpus,
        used,
        wine: wine(),
    }
}

/// Every adapter that is not the software one, and which of them a run would
/// use: the one the default device is made on.
fn adapters() -> (Vec<Gpu>, Option<usize>) {
    // SAFETY: plain COM calls; every object made here is released as it goes.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return (Vec::new(), None);
        };
        // The adapter a run gets: whatever the default device is made on.
        let mut default: Option<ID3D12Device> = None;
        let luid = if D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut default).is_ok() {
            default.as_ref().map(|d| d.GetAdapterLuid())
        } else {
            None
        };
        let mut gpus = Vec::new();
        let mut used = None;
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else {
                break;
            };
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let described = utf16(&desc.Description);
            let name = described.trim_end();
            // A laptop with two display paths lists the one card twice, under
            // two Luids: the report names it once.
            if gpus
                .iter()
                .any(|g: &Gpu| g.name == name && g.device == desc.DeviceId)
            {
                continue;
            }
            let mut gpu = Gpu {
                name: name.to_owned(),
                vendor: desc.VendorId,
                device: desc.DeviceId,
                driver: driver_version(&adapter),
                dedicated: desc.DedicatedVideoMemory as u64,
                shared: desc.SharedSystemMemory as u64,
                ..Gpu::default()
            };
            if let Some(device) = device_on(&adapter) {
                caps(&device, &mut gpu);
            }
            if luid.is_some_and(|l| {
                l.LowPart == desc.AdapterLuid.LowPart && l.HighPart == desc.AdapterLuid.HighPart
            }) {
                used = Some(gpus.len());
            }
            gpus.push(gpu);
        }
        // No LUID to match (the default device may be the software one): the
        // first adapter that makes a device of feature level 12.0 is the one a
        // run would use.
        if used.is_none() {
            used = gpus.iter().position(|g| g.shader_model.is_some());
        }
        (gpus, used)
    }
}

/// A device of the feature level the runtime asks for, on `adapter`.
fn device_on(adapter: &IDXGIAdapter1) -> Option<ID3D12Device> {
    let unknown: IUnknown = adapter.cast().ok()?;
    let mut device: Option<ID3D12Device> = None;
    // SAFETY: plain COM call; the device is released by its owner.
    unsafe { D3D12CreateDevice(Some(&unknown), D3D_FEATURE_LEVEL_12_0, &mut device) }.ok()?;
    device
}

fn driver_version(adapter: &IDXGIAdapter1) -> Option<[u16; 4]> {
    // SAFETY: plain COM call.
    let version = unsafe { adapter.CheckInterfaceSupport(&IDXGIDevice::IID) }.ok()? as u64;
    Some([
        (version >> 48) as u16,
        (version >> 32) as u16,
        (version >> 16) as u16,
        version as u16,
    ])
}

/// What a device says it can do, and the largest shader-visible view heap it
/// makes. The heap is the question `descriptor/host.rs` asks once per
/// run (`probe_view_limit`), so a report answers it with the same number.
fn caps(device: &ID3D12Device, gpu: &mut Gpu) {
    let mut options1 = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
    if supports(device, D3D12_FEATURE_D3D12_OPTIONS1, &mut options1) {
        gpu.wave_min = options1.WaveLaneCountMin;
        gpu.wave_max = options1.WaveLaneCountMax;
    }
    let mut model = D3D12_FEATURE_DATA_SHADER_MODEL {
        HighestShaderModel: D3D_SHADER_MODEL_6_9,
    };
    if supports(device, D3D12_FEATURE_SHADER_MODEL, &mut model) {
        // The call answers with the highest it takes of the one asked for:
        // 6.0 is 100, so 103 is 6.3.
        gpu.shader_model = Some((6, model.HighestShaderModel.0.saturating_sub(100) as u32));
    }
    let mut options7 = D3D12_FEATURE_DATA_D3D12_OPTIONS7::default();
    if supports(device, D3D12_FEATURE_D3D12_OPTIONS7, &mut options7) {
        gpu.mesh_shaders = options7.MeshShaderTier != D3D12_MESH_SHADER_TIER_NOT_SUPPORTED;
    }
    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
    if supports(device, D3D12_FEATURE_D3D12_OPTIONS, &mut options) {
        gpu.binding_tier = options.ResourceBindingTier.0 as u32;
    }
    let mut options16 = D3D12_FEATURE_DATA_D3D12_OPTIONS16::default();
    if supports(device, D3D12_FEATURE_D3D12_OPTIONS16, &mut options16) {
        gpu.upload_heap = options16.GPUUploadHeapSupported.as_bool();
    }
    gpu.view_heap = view_heap(device);
}

fn supports<T>(
    device: &ID3D12Device,
    feature: windows::Win32::Graphics::Direct3D12::D3D12_FEATURE,
    data: &mut T,
) -> bool {
    // SAFETY: `data` is the structure that belongs to `feature`.
    unsafe {
        device.CheckFeatureSupport(
            feature,
            std::ptr::from_mut(data).cast(),
            size_of::<T>() as u32,
        )
    }
    .is_ok()
}

/// The most views a shader-visible heap this device makes holds, of the sizes
/// the runtime asks about (`probe_view_limit`): 0 when even the title's own
/// does not fit.
fn view_heap(device: &ID3D12Device) -> u32 {
    [MOST_VIEWS, MORE_VIEWS, TITLE_VIEWS]
        .into_iter()
        .find(|&views| {
            // SAFETY: plain heap creation; the heap is released at once.
            unsafe {
                device
                    .CreateDescriptorHeap::<ID3D12DescriptorHeap>(&D3D12_DESCRIPTOR_HEAP_DESC {
                        Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                        NumDescriptors: views,
                        Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                        NodeMask: 0,
                    })
                    .is_ok()
            }
        })
        .unwrap_or(0)
}

fn cpu() -> Cpu {
    let mut info = SYSTEM_INFO::default();
    // SAFETY: plain call; the structure is filled in but for its union, which
    // this does not read.
    unsafe { GetSystemInfo(&mut info) };
    Cpu {
        // The registry pads the name with spaces to a fixed width.
        name: registry_string(
            "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
            "ProcessorNameString",
        )
        .map_or_else(
            || "unknown processor".to_owned(),
            |name| name.trim().to_owned(),
        ),
        threads: info.dwNumberOfProcessors,
        // SAFETY: plain call.
        avx2: unsafe { IsProcessorFeaturePresent(PF_AVX2_INSTRUCTIONS_AVAILABLE) }.as_bool(),
    }
}

fn memory() -> Memory {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: plain call; the structure is filled in.
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return Memory::default();
    }
    Memory {
        total: status.ullTotalPhys,
        free: status.ullAvailPhys,
        // The commit limit is what the title finds when it asks for its 43
        // GiB: physical memory plus the paging file.
        commit_limit: status.ullTotalPageFile,
        commit_free: status.ullAvailPageFile,
    }
}

/// Whether this is Wine rather than Windows: `ntdll` there exports
/// `wine_get_version`, and the title's own anti-tamper ends the run on it
/// (#6, #19).
fn wine() -> bool {
    // SAFETY: plain loader calls; nothing is called through what is found.
    unsafe {
        let Ok(ntdll) = GetModuleHandleW(w!("ntdll.dll")) else {
            return false;
        };
        GetProcAddress(ntdll, s!("wine_get_version")).is_some()
    }
}

fn windows_version() -> String {
    const KEY: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let build = registry_string(KEY, "CurrentBuild");
    let update = registry_dword(KEY, "UBR");
    match build {
        // Windows 11 starts at build 22000; the product name says "Windows
        // 10" on 11 (it is there for compatibility), so the build decides.
        Some(build) => {
            let major = build
                .trim()
                .parse::<u32>()
                .map_or(10, |b| if b >= 22000 { 11 } else { 10 });
            match update {
                Some(update) => format!("Windows {major}, build {build}.{update}"),
                None => format!("Windows {major}, build {build}"),
            }
        }
        None => "Windows, of an unknown version".to_owned(),
    }
}

/// A string value under `HKEY_LOCAL_MACHINE`, for the few facts Windows only
/// keeps there.
fn registry_string(key: &str, name: &str) -> Option<String> {
    let (kind, value) = registry_value(key, name)?;
    if kind != REG_SZ {
        return None;
    }
    let units: Vec<u16> = value
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// A `REG_DWORD` under `HKEY_LOCAL_MACHINE`: the update number of a build is
/// kept as one, so `CurrentBuild` alone says 26100 where the report says
/// 26100.2314.
fn registry_dword(key: &str, name: &str) -> Option<u32> {
    let (kind, value) = registry_value(key, name)?;
    if kind != REG_DWORD {
        return None;
    }
    let four: [u8; 4] = value.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(four))
}

/// The value of `name` under `HKEY_LOCAL_MACHINE\key`, and what kind it is.
fn registry_value(key: &str, name: &str) -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
    let key = wide(key);
    let name = wide(name);
    // SAFETY: the strings are NUL-terminated wide strings that outlive the
    // calls, and the key is closed on every path that opened it.
    unsafe {
        let mut handle = HKEY::default();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            None,
            KEY_READ,
            &mut handle,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let mut kind = REG_VALUE_TYPE::default();
        let mut bytes = 0u32;
        let asked = RegQueryValueExW(
            handle,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut bytes),
        );
        if asked != ERROR_SUCCESS || bytes == 0 {
            let _ = RegCloseKey(handle);
            return None;
        }
        let mut value = vec![0u8; bytes as usize];
        let read = RegQueryValueExW(
            handle,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(value.as_mut_ptr()),
            Some(&mut bytes),
        );
        let _ = RegCloseKey(handle);
        if read != ERROR_SUCCESS {
            return None;
        }
        value.truncate(bytes as usize);
        Some((kind, value))
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn utf16(text: &[u16]) -> String {
    let end = text
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}
