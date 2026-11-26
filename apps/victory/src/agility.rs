// Based on Marvisak/VIctory PR #175 (19aec59).
#![allow(non_upper_case_globals)]

#[unsafe(no_mangle)]
pub static D3D12SDKVersion: u32 = 620;

#[repr(transparent)]
pub struct SdkPath(*const std::ffi::c_char);

// SAFETY: the pointer refers to a static, immutable C string.
unsafe impl Sync for SdkPath {}

#[unsafe(no_mangle)]
pub static D3D12SDKPath: SdkPath = SdkPath(c".\\D3D12\\".as_ptr());
