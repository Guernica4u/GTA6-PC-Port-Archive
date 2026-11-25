//! What the host GPU offers, without a title: the adapter `run` would pick
//! and the capabilities that decide what of the title's work it can run.
//! Read by `victory system` for test reports.

use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::Interface;

use crate::descriptor::host::{DYNAMIC, DYNAMIC_CAPPED, PORTABLE_VIEWS};

/// One adapter Windows lists, in its order of preference for speed.
pub struct Adapter {
    pub name: String,
    pub vendor: u32,
    pub device: u32,
    pub dedicated_mb: u64,
    pub shared_mb: u64,
    /// The user-mode driver's version, `a.b.c.d`.
    pub driver: Option<String>,
    pub software: bool,
}

/// The title's view heap on this host.
pub enum ViewHeap {
    /// All the slots the title and we use fit.
    Fits,
    /// Capped at [`PORTABLE_VIEWS`]: `fit` of the title's `wanted` slots
    /// are made; tables reaching past them are remapped.
    Capped { wanted: u32, fit: u32 },
    /// Not even a capped heap could be made.
    None,
}

/// The device made on the first adapter, as `run` makes it.
pub struct Device {
    /// Highest shader model, e.g. `(6, 8)`.
    pub shader_model: (u32, u32),
    pub mesh_shaders: bool,
    /// Wave sizes, `(min, max)` lanes; `None` if wave operations are not
    /// supported.
    pub wave_lanes: Option<(u32, u32)>,
    pub resource_binding_tier: u32,
    pub view_heap: ViewHeap,
    pub gpu_upload_heap: bool,
}

pub struct Report {
    pub adapters: Vec<Adapter>,
    /// On `adapters[0]`, or why it could not be made.
    pub device: Result<Device, String>,
}

/// The adapters and what a device on the first can do. Creates its own
/// device (not the one a title uses) and releases it.
pub fn report() -> windows::core::Result<Report> {
    // SAFETY: plain DXGI queries.
    let adapters: Vec<IDXGIAdapter1> = unsafe {
        let factory: IDXGIFactory6 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        (0..)
            .map_while(|i| {
                factory
                    .EnumAdapterByGpuPreference(i, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)
                    .ok()
            })
            .collect()
    };
    let device = match adapters.first() {
        Some(first) => device(first),
        None => Err("no adapter".into()),
    };
    Ok(Report {
        adapters: adapters.iter().filter_map(|a| describe(a).ok()).collect(),
        device,
    })
}

fn describe(adapter: &IDXGIAdapter1) -> windows::core::Result<Adapter> {
    // SAFETY: plain DXGI queries.
    let (desc, umd) = unsafe {
        (
            adapter.GetDesc1()?,
            adapter.CheckInterfaceSupport(&IDXGIDevice::IID).ok(),
        )
    };
    let end = desc
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.Description.len());
    Ok(Adapter {
        name: String::from_utf16_lossy(&desc.Description[..end]),
        vendor: desc.VendorId,
        device: desc.DeviceId,
        dedicated_mb: (desc.DedicatedVideoMemory >> 20) as u64,
        shared_mb: (desc.SharedSystemMemory >> 20) as u64,
        driver: umd.map(|v| {
            let v = v as u64;
            format!(
                "{}.{}.{}.{}",
                v >> 48,
                (v >> 32) & 0xffff,
                (v >> 16) & 0xffff,
                v & 0xffff
            )
        }),
        software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
    })
}

fn device(adapter: &IDXGIAdapter1) -> Result<Device, String> {
    let mut device: Option<ID3D12Device> = None;
    // SAFETY: device creation on an adapter DXGI listed.
    unsafe { D3D12CreateDevice(adapter, D3D_FEATURE_LEVEL_12_0, &mut device) }
        .map_err(|e| format!("no D3D12 device at feature level 12_0: {e}"))?;
    let device = device.ok_or("no D3D12 device")?;
    let query = |feature, data: *mut core::ffi::c_void, size: usize| {
        // SAFETY: each caller passes its own structure of `size` bytes.
        unsafe { device.CheckFeatureSupport(feature, data, size as u32) }.is_ok()
    };

    // The runtime refuses a model it does not know: ask from the highest
    // down.
    let mut shader_model = (0, 0);
    for model in (D3D_SHADER_MODEL_6_0.0..=D3D_SHADER_MODEL_6_9.0).rev() {
        let mut data = D3D12_FEATURE_DATA_SHADER_MODEL {
            HighestShaderModel: D3D_SHADER_MODEL(model),
        };
        if query(
            D3D12_FEATURE_SHADER_MODEL,
            (&raw mut data).cast(),
            size_of_val(&data),
        ) {
            let m = data.HighestShaderModel.0 as u32;
            shader_model = (m >> 4, m & 0xf);
            break;
        }
    }

    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
    query(
        D3D12_FEATURE_D3D12_OPTIONS,
        (&raw mut options).cast(),
        size_of_val(&options),
    );
    let mut options1 = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
    let waves = query(
        D3D12_FEATURE_D3D12_OPTIONS1,
        (&raw mut options1).cast(),
        size_of_val(&options1),
    ) && options1.WaveOps.as_bool()
        && options1.WaveLaneCountMin > 0;
    let mut options7 = D3D12_FEATURE_DATA_D3D12_OPTIONS7::default();
    let mesh = query(
        D3D12_FEATURE_D3D12_OPTIONS7,
        (&raw mut options7).cast(),
        size_of_val(&options7),
    ) && options7.MeshShaderTier != D3D12_MESH_SHADER_TIER_NOT_SUPPORTED;
    let mut options16 = D3D12_FEATURE_DATA_D3D12_OPTIONS16::default();
    let upload = query(
        D3D12_FEATURE_D3D12_OPTIONS16,
        (&raw mut options16).cast(),
        size_of_val(&options16),
    ) && options16.GPUUploadHeapSupported.as_bool();

    Ok(Device {
        shader_model,
        mesh_shaders: mesh && shader_model >= (6, 6),
        wave_lanes: waves.then_some((options1.WaveLaneCountMin, options1.WaveLaneCountMax)),
        resource_binding_tier: options.ResourceBindingTier.0 as u32,
        view_heap: view_heap(&device),
        gpu_upload_heap: upload,
    })
}

/// Makes the view heap `run` asks for, then the capped one it falls back to
/// (`descriptor::host::HostHeap::new`), and drops it.
fn view_heap(device: &ID3D12Device) -> ViewHeap {
    let fits = |count: u32| {
        // SAFETY: plain heap creation; the heap is released at once.
        unsafe {
            device.CreateDescriptorHeap::<ID3D12DescriptorHeap>(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: count,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                NodeMask: 0,
            })
        }
        .is_ok()
    };
    let all = victory_shader::emit::VIEW_SLOTS;
    let wanted = all - DYNAMIC - crate::memory::SLOTS;
    if fits(all) {
        ViewHeap::Fits
    } else if fits(PORTABLE_VIEWS) {
        ViewHeap::Capped {
            wanted,
            fit: PORTABLE_VIEWS - DYNAMIC_CAPPED - crate::memory::SLOTS,
        }
    } else {
        ViewHeap::None
    }
}
