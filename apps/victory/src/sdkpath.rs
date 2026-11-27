//! Agility SDK exports, so Windows 10 gets shader model 6.6.
//!
//! `d3d12.dll` loads an app-local `D3D12Core.dll` only when the executable
//! exports `D3D12SDKVersion` and `D3D12SDKPath`. Without them Windows 10's
//! in-box runtime refuses directly indexed descriptor heaps, every host
//! pipeline is refused, and the title crashes reading a null pipeline.
//!
//! Put `D3D12Core.dll` (and optionally `d3d12SDKLayers.dll`) from the
//! Microsoft.Direct3D.D3D12 1.620.x NuGet package (`build\native\bin\x64\`)
//! in a `D3D12` folder next to `victory.exe`. The exports are added to the
//! binary in `build.rs`.

/// Agility SDK version the runtime must match (1.620.x).
#[cfg(windows)]
#[allow(non_upper_case_globals)]
#[used]
#[unsafe(no_mangle)]
pub static D3D12SDKVersion: u32 = 620;

/// Folder, relative to the executable, that holds `D3D12Core.dll`.
#[cfg(windows)]
#[allow(non_upper_case_globals)]
#[used]
#[unsafe(no_mangle)]
pub static D3D12SDKPath: &[u8; 9] = b".\\D3D12\\\0";
