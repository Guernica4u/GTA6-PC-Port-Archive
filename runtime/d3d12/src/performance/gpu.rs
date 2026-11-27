//! Graphics card sensors, which D3D12 does not report. Two sources, chosen by
//! what answers for the adapter: the NVIDIA management library (`nvml.dll`,
//! from the driver on Windows and from Proton's wine-nvml), and the sensor
//! files a Linux kernel driver such as amdgpu publishes, read through Wine.

use super::stats::{Gpu, pci_id, scaled, wine_nvml_path};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW, LoadLibraryW,
};
use windows::core::{PCSTR, PCWSTR, s, w};

pub(super) enum Sensors {
    Nvml(Nvml),
    /// A DRM card's `device` directory.
    Sysfs(PathBuf),
}

impl Sensors {
    /// Sensors for the adapter with these PCI identifiers.
    pub(super) fn open(vendor: u32, device: u32) -> Option<Self> {
        let found = Nvml::open(vendor, device).map(Self::Nvml)
            .or_else(|| sysfs(vendor, device).map(Self::Sysfs));
        crate::diagnostics::log(&format!(
            "graphics card sensors for {vendor:04x}:{device:04x}: {}",
            match &found {
                Some(Self::Nvml(_)) => "NVML",
                Some(Self::Sysfs(_)) => "Linux sysfs",
                None => "none",
            }
        ));
        found
    }

    pub(super) fn read(&self) -> Gpu {
        match self {
            Self::Nvml(nvml) => nvml.read(),
            Self::Sysfs(card) => {
                let value = |path: &Path, unit| std::fs::read_to_string(path).ok().and_then(|text| scaled(&text, unit));
                let monitor = std::fs::read_dir(card.join("hwmon")).ok()
                    .and_then(|entries| entries.flatten().next()).map(|entry| entry.path());
                let sensor = |name: &str, unit| monitor.as_ref().and_then(|m| value(&m.join(name), unit));
                Gpu {
                    load: value(&card.join("gpu_busy_percent"), 1.0),
                    celsius: sensor("temp1_input", 1e3),
                    megahertz: sensor("freq1_input", 1e6),
                    watts: sensor("power1_average", 1e6).or_else(|| sensor("power1_input", 1e6)),
                }
            }
        }
    }
}

fn sysfs(vendor: u32, device: u32) -> Option<PathBuf> {
    // Wine maps the host's root to Z:. Elsewhere the path does not exist.
    (0..16).map(|card| PathBuf::from(format!(r"Z:\sys\class\drm\card{card}\device"))).find(|card| {
        let id = |name| std::fs::read_to_string(card.join(name)).ok().and_then(|text| pci_id(&text));
        // NVIDIA's driver lists its cards here without any of these sensors.
        id("vendor") == Some(vendor) && id("device") == Some(device)
            && (card.join("gpu_busy_percent").exists() || card.join("hwmon").exists())
    })
}

/// wine-nvml from the directories Proton lists in `WINEDLLPATH`.
fn wine_nvml() -> Option<HMODULE> {
    let listed = std::env::var("WINEDLLPATH").ok()?;
    listed.split(':').filter_map(wine_nvml_path).find_map(|path| {
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        // SAFETY: the name is terminated; the file belongs to the Wine runtime.
        unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) }.ok()
    })
}

/// `nvmlPciInfo_t`.
#[repr(C)]
struct PciInfo {
    legacy_bus_id: [u8; 16],
    domain: u32,
    bus: u32,
    device: u32,
    /// Device identifier in the high half, vendor in the low half.
    device_and_vendor: u32,
    subsystem: u32,
    bus_id: [u8; 32],
}

type Handle = *mut c_void;
/// A query with one 32-bit result.
type Query = unsafe extern "C" fn(Handle, *mut u32) -> u32;
/// A query for one of several 32-bit results, chosen by a selector.
type Select = unsafe extern "C" fn(Handle, u32, *mut u32) -> u32;

/// The functions of one NVML device. The library stays loaded for the process.
pub(super) struct Nvml {
    device: Handle,
    utilization: Option<unsafe extern "C" fn(Handle, *mut [u32; 2]) -> u32>,
    temperature: Option<Select>,
    clock: Option<Select>,
    power: Option<Query>,
}

impl Nvml {
    fn open(vendor: u32, device: u32) -> Option<Self> {
        // SAFETY: the library is loaded from the system directory only. Each
        // function is transmuted to its documented NVML signature, and every
        // output points at a local of the documented layout.
        unsafe {
            // Proton's wine-nvml may have no file in the prefix's system
            // directory, only an entry on Wine's DLL path.
            let library = LoadLibraryExW(w!("nvml.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
                .ok()
                .or_else(wine_nvml)?;
            let find = |name: PCSTR| GetProcAddress(library, name);
            macro_rules! function {
                ($name:literal) => {
                    find(s!($name)).map(|f| std::mem::transmute::<_, _>(f))
                };
            }
            let init: unsafe extern "C" fn() -> u32 = function!("nvmlInit_v2")?;
            let count: unsafe extern "C" fn(*mut u32) -> u32 = function!("nvmlDeviceGetCount_v2")?;
            let by_index: unsafe extern "C" fn(u32, *mut Handle) -> u32 = function!("nvmlDeviceGetHandleByIndex_v2")?;
            let pci: unsafe extern "C" fn(Handle, *mut PciInfo) -> u32 = function!("nvmlDeviceGetPciInfo_v3")?;
            if init() != 0 {
                return None;
            }
            let mut devices = 0;
            if count(&mut devices) != 0 {
                return None;
            }
            let wanted = device << 16 | vendor;
            let found = (0..devices).find_map(|index| {
                let mut handle: Handle = std::ptr::null_mut();
                let mut info: PciInfo = std::mem::zeroed();
                (by_index(index, &mut handle) == 0 && pci(handle, &mut info) == 0 && info.device_and_vendor == wanted)
                    .then_some(handle)
            })?;
            Some(Self {
                device: found,
                utilization: function!("nvmlDeviceGetUtilizationRates"),
                temperature: function!("nvmlDeviceGetTemperature"),
                clock: function!("nvmlDeviceGetClockInfo"),
                power: function!("nvmlDeviceGetPowerUsage"),
            })
        }
    }

    fn read(&self) -> Gpu {
        // SAFETY: the handle came from this library, and each output is a local.
        unsafe {
            let select = |query: Option<Select>| {
                let mut value = 0;
                // Selector 0 is the card's own sensor and the graphics clock.
                query.filter(|query| query(self.device, 0, &mut value) == 0).map(|_| value as f32)
            };
            let load = self.utilization.and_then(|query| {
                let mut rates = [0; 2];
                (query(self.device, &mut rates) == 0).then_some(rates[0] as f32)
            });
            let watts = self.power.and_then(|query| {
                let mut milliwatts = 0;
                (query(self.device, &mut milliwatts) == 0).then(|| milliwatts as f32 / 1000.0)
            });
            Gpu { load, celsius: select(self.temperature), megahertz: select(self.clock), watts }
        }
    }
}
