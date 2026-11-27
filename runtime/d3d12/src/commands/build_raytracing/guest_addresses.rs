//! Guest addresses in acceleration structure commands.
//!
//! Geometry, scratch, instance records and size reports can lie in a title's
//! own memory, which the host reaches only through an imported buffer, as for
//! any view of that memory. Native addresses pass through unchanged.
use super::super::{Recording, raytracing::retain};
use windows::Win32::Foundation::{E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY};
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};
use windows::core::{IUnknown, Interface, Result};

/// The native address of `bytes` at `address`. The list retains any import.
pub(in crate::commands) fn native(
    state: &Recording,
    owner: &IUnknown,
    address: u64,
    bytes: u64,
    flags: D3D12_RESOURCE_FLAGS,
) -> Result<u64> {
    Ok(imported(state, owner, address, bytes, flags)?.unwrap_or(address))
}

/// The native address of `bytes` at `address` if a buffer backs it there,
/// or None for an address the host already knows.
pub(in crate::commands) fn imported(
    state: &Recording,
    owner: &IUnknown,
    address: u64,
    bytes: u64,
    flags: D3D12_RESOURCE_FLAGS,
) -> Result<Option<u64>> {
    if address == 0 || bytes == 0 {
        return Ok(None);
    }
    let (buffer, native) = unsafe {
        crate::device::command_buffer_address_flags(owner.as_raw(), address, bytes, flags)
    }?;
    let Some(buffer) = buffer else { return Ok(None) };
    retain(state, &[&buffer])?;
    Ok(Some(native))
}

fn span(count: u64, stride: u64, element: u64) -> Option<u64> {
    if count == 0 {
        return Some(0);
    }
    (count - 1).checked_mul(stride)?.checked_add(element)
}

fn vertex_bytes(format: DXGI_FORMAT) -> u64 {
    match format {
        DXGI_FORMAT_R32G32B32_FLOAT => 12,
        DXGI_FORMAT_R32G32_FLOAT
        | DXGI_FORMAT_R16G16B16A16_FLOAT
        | DXGI_FORMAT_R16G16B16A16_SNORM
        | DXGI_FORMAT_R16G16B16A16_UNORM => 8,
        DXGI_FORMAT_R8G8_UNORM | DXGI_FORMAT_R8G8_SNORM => 2,
        _ => 4,
    }
}

/// Bottom-level geometry with every buffer address made native. The build
/// must be recorded while the returned descriptions are alive.
pub(super) fn geometry(
    state: &Recording,
    owner: &IUnknown,
    inputs: &D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS,
) -> Result<Vec<D3D12_RAYTRACING_GEOMETRY_DESC>> {
    let count = inputs.NumDescs as usize;
    let mut descs = Vec::new();
    descs.try_reserve_exact(count).map_err(|_| E_OUTOFMEMORY)?;
    for index in 0..count {
        // SAFETY: `ray_build::valid` checked the layout and every pointer.
        let mut geometry = unsafe {
            if inputs.DescsLayout == D3D12_ELEMENTS_LAYOUT_ARRAY {
                *inputs.Anonymous.pGeometryDescs.add(index)
            } else {
                *(*inputs.Anonymous.ppGeometryDescs.add(index)).as_ref().ok_or(E_INVALIDARG)?
            }
        };
        match geometry.Type {
            D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES => {
                // SAFETY: the descriptor's discriminant selects Triangles.
                let triangles = unsafe { &mut geometry.Anonymous.Triangles };
                let vertices = &mut triangles.VertexBuffer;
                let bytes = span(triangles.VertexCount.into(), vertices.StrideInBytes, vertex_bytes(triangles.VertexFormat))
                    .ok_or(E_INVALIDARG)?;
                vertices.StartAddress = native(state, owner, vertices.StartAddress, bytes, D3D12_RESOURCE_FLAG_NONE)?;
                let index_bytes = match triangles.IndexFormat {
                    DXGI_FORMAT_R16_UINT => 2,
                    DXGI_FORMAT_R32_UINT => 4,
                    _ => 0,
                };
                let bytes = u64::from(triangles.IndexCount) * index_bytes;
                triangles.IndexBuffer = native(state, owner, triangles.IndexBuffer, bytes, D3D12_RESOURCE_FLAG_NONE)?;
                triangles.Transform3x4 = native(state, owner, triangles.Transform3x4, 48, D3D12_RESOURCE_FLAG_NONE)?;
            }
            D3D12_RAYTRACING_GEOMETRY_TYPE_PROCEDURAL_PRIMITIVE_AABBS => {
                // SAFETY: the descriptor's discriminant selects AABBs.
                let boxes = unsafe { &mut geometry.Anonymous.AABBs };
                let bytes = span(boxes.AABBCount, boxes.AABBs.StrideInBytes, 24).ok_or(E_INVALIDARG)?;
                boxes.AABBs.StartAddress = native(state, owner, boxes.AABBs.StartAddress, bytes, D3D12_RESOURCE_FLAG_NONE)?;
            }
            _ => return Err(E_NOTIMPL.into()),
        }
        descs.push(geometry);
    }
    Ok(descs)
}

/// Size reports written to native addresses. Each report is 8 bytes per structure.
pub(in crate::commands) fn reports(
    state: &Recording,
    owner: &IUnknown,
    outputs: &mut [D3D12_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO_DESC],
    structures: u64,
) -> Result<()> {
    for output in outputs {
        let flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
        output.DestBuffer = native(state, owner, output.DestBuffer, structures * 8, flags)?;
    }
    Ok(())
}
