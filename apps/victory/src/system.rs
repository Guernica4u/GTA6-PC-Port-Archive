//! `victory system`: this machine as test reports need it, without a title.
//!
//! What decides how far the title gets on a host: the CPU (AVX2), memory
//! and Windows commit, Windows itself (or Wine), and the GPU `run` would
//! pick, with the capabilities translated work depends on. The output is
//! meant to be pasted into a report whole.

use anyhow::Result;
use victory_gpu::report::{Device, ViewHeap};

pub fn show() -> Result<()> {
    let mut warnings = Vec::new();
    println!("VIctory {}, {} build", crate::VERSION, build_kind());
    let wine = wine_version();
    match &wine {
        Some(w) => println!("Windows {} (Wine {w})", victory_loader::windows_version()),
        None => println!("Windows {}", victory_loader::windows_version()),
    }

    let avx2 = std::arch::is_x86_feature_detected!("avx2");
    println!("CPU: {}, AVX2 {}", victory_loader::cpu_name(), yes_no(avx2));
    if !avx2 {
        warnings.push("the CPU has no AVX2, which the title's code uses".to_owned());
    }

    if let Some(m) = memory() {
        println!(
            "Memory: {:.1} GiB RAM ({:.1} free), commit {:.1} GiB free of {:.1}",
            gib(m.ullTotalPhys),
            gib(m.ullAvailPhys),
            gib(m.ullAvailPageFile),
            gib(m.ullTotalPageFile),
        );
    }

    let report = match victory_gpu::report::report() {
        Ok(report) => report,
        Err(e) => {
            println!("GPU: DXGI lists no adapters ({e})");
            return Ok(());
        }
    };
    // The software adapter every Windows lists is left out unless used.
    for (i, a) in report.adapters.iter().enumerate() {
        if i > 0 && a.software {
            continue;
        }
        println!(
            "{} {} [{:04x}:{:04x}], driver {}, {} MiB dedicated, {} MiB shared{}",
            if i == 0 {
                "GPU (used):"
            } else {
                "GPU (other):"
            },
            a.name,
            a.vendor,
            a.device,
            a.driver.as_deref().unwrap_or("unknown"),
            a.dedicated_mb,
            a.shared_mb,
            if a.software { ", software" } else { "" },
        );
        if i == 0 {
            match &report.device {
                Ok(device) => println!("  {}", describe(device)),
                Err(e) => println!("  {e}"),
            }
        }
    }
    if report.adapters.first().is_some_and(|a| a.software) {
        warnings.push("the GPU used is a software adapter: frames take seconds".to_owned());
    }
    match &report.device {
        Ok(device) => warnings.extend(problems(device)),
        Err(_) => warnings.push("no D3D12 device: the title cannot draw".to_owned()),
    }
    if warnings.is_empty() {
        println!("No known blockers.");
    } else {
        println!("Known problems on this host:");
        for w in warnings {
            println!("- {w}");
        }
    }
    Ok(())
}

/// The device's capabilities on one line.
fn describe(d: &Device) -> String {
    let waves = d
        .wave_lanes
        .map_or("no wave operations".to_owned(), |(min, max)| {
            if min == max {
                format!("waves of {min} lanes")
            } else {
                format!("waves of {min} to {max} lanes")
            }
        });
    let heap = match d.view_heap {
        ViewHeap::Fits => "fits".to_owned(),
        ViewHeap::Capped { wanted, fit } => format!("capped, {fit} of the title's {wanted} slots"),
        ViewHeap::None => "cannot be made".to_owned(),
    };
    format!(
        "shader model {}.{}, mesh shaders {}, {waves}, resource binding tier {}, view heap {heap}, \
         GPU upload heap {}",
        d.shader_model.0,
        d.shader_model.1,
        yes_no(d.mesh_shaders),
        d.resource_binding_tier,
        yes_no(d.gpu_upload_heap),
    )
}

/// What of the title's work the device cannot run, as `run` would find it.
fn problems(d: &Device) -> Vec<String> {
    let mut problems = Vec::new();
    if d.shader_model < (6, 6) {
        problems.push(format!(
            "shader model {}.{}: translated shaders need 6.6",
            d.shader_model.0, d.shader_model.1
        ));
    }
    if !d.mesh_shaders {
        problems.push("no mesh shaders at shader model 6.6: the world is not drawn".to_owned());
    }
    match d.wave_lanes {
        Some((min, max)) if !(min..=max).contains(&64) => problems.push(format!(
            "no 64-lane waves (only {min} to {max}): the console's wave64 compute programs \
             that read other lanes are not translated, and their dispatches are dropped"
        )),
        None => problems.push("no wave operations".to_owned()),
        _ => {}
    }
    match d.view_heap {
        ViewHeap::Capped { wanted, fit } => problems.push(format!(
            "the view heap is capped: {} of the title's descriptor slots do not fit, and \
             tables reaching them are remapped",
            wanted - fit
        )),
        ViewHeap::None => problems.push("no shader-visible view heap could be made".to_owned()),
        ViewHeap::Fits => {}
    }
    problems
}

fn build_kind() -> String {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    if cfg!(feature = "diagnostics") {
        format!("{profile}, diagnostics")
    } else {
        profile.to_owned()
    }
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / f64::from(1u32 << 30)
}

fn memory() -> Option<windows_sys::Win32::System::SystemInformation::MEMORYSTATUSEX> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: a zeroed MEMORYSTATUSEX is valid; its length field is set.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: fills the structure passed, whose size it was told.
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status)
}

/// Wine's version when running under it (Proton too): its ntdll exports
/// `wine_get_version`.
fn wine_version() -> Option<String> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    // SAFETY: ntdll is loaded in every process; the name is NUL-terminated.
    let ntdll = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
    if ntdll.is_null() {
        return None;
    }
    // SAFETY: looks up an export by a NUL-terminated name.
    let get = unsafe { GetProcAddress(ntdll, c"wine_get_version".as_ptr().cast()) }?;
    // SAFETY: Wine's `const char *wine_get_version(void)`, a static string.
    let version = unsafe {
        let get: unsafe extern "C" fn() -> *const std::ffi::c_char = std::mem::transmute(get);
        std::ffi::CStr::from_ptr(get())
    };
    Some(version.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Device {
        Device {
            shader_model: (6, 8),
            mesh_shaders: true,
            wave_lanes: Some((32, 64)),
            resource_binding_tier: 3,
            view_heap: ViewHeap::Fits,
            gpu_upload_heap: false,
        }
    }

    #[test]
    fn a_wave64_tier_3_device_has_no_problems() {
        assert!(problems(&device()).is_empty());
    }

    #[test]
    fn wave32_only_hosts_are_told_wave64_programs_are_dropped() {
        let d = Device {
            wave_lanes: Some((32, 32)),
            ..device()
        };
        let found = problems(&d);
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("no 64-lane waves (only 32 to 32)"));
    }

    #[test]
    fn a_capped_view_heap_names_the_slots_left_out() {
        let d = Device {
            view_heap: ViewHeap::Capped {
                wanted: 1_245_184,
                fit: 933_440,
            },
            ..device()
        };
        assert!(problems(&d)[0].contains("311744 of the title's descriptor slots"));
        assert!(describe(&d).contains("view heap capped, 933440 of the title's 1245184 slots"));
    }

    #[test]
    fn no_mesh_shaders_below_shader_model_6_6() {
        let d = Device {
            shader_model: (6, 5),
            mesh_shaders: false,
            ..device()
        };
        assert_eq!(problems(&d).len(), 2);
    }
}
