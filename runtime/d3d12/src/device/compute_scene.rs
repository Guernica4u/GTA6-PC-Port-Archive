//! Private host root bindings for translated Lumen inline ray queries.
use core::{fmt::Write, ops::Range};
use std::sync::{Arc, Once};
use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER};
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::{Interface, Result};

// A translated compute program opts into a private root ABI by declaring
// native TLAS SRVs in a space the title does not use. Two layouts exist:
// - Space 1 holds one scene at t0 for a title that binds only space 0. A hit
//   that needs face normals also declares raw t1 and t2 in that space.
// - SCENE_SPACE holds one scene per title acceleration structure. Register i
//   is the title's i-th structure, ordered by (space, register). A translation
//   declares only the scenes it reads.
// Match the DXIL actually supplied to the driver.
pub(crate) const SCENE_SPACE: u32 = 900;
const MAX_SCENES: usize = 8;

pub(crate) struct InlineProgram {
    pub(crate) source_hash: String,
    pub(crate) triangle_geometry: bool,
    pub(crate) first_dispatch: Once,
}

// PSVResourceBindInfo1 has six DWORDs: type, space, lower, upper, kind, flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) class: u32,
    pub(crate) space: u32,
    pub(crate) lower: u32,
    pub(crate) upper: u32,
    pub(crate) kind: u32,
}

const RAW_SRV: u32 = 4;
const KIND_RAW: u32 = 11;
const KIND_SCENE: u32 = 16;

impl Binding {
    fn scene(&self) -> bool {
        self.kind == KIND_SCENE
    }
    fn single(&self) -> bool {
        self.class == RAW_SRV && self.lower == self.upper
    }
}

fn bindings(shader: &[u8]) -> Option<Vec<Binding>> {
    let directory = tuxbox_dxbc::Dxbc::parse(shader).ok()?;
    let part = directory.chunk(b"PSV0")?;
    let psv = shader.get(part.offset..part.offset.checked_add(part.size)?)?;
    let after_runtime = psv.get(4usize.checked_add(word(psv, 0)? as usize)?..)?;
    let count = word(after_runtime, 0)? as usize;
    let stride = word(after_runtime, 4)? as usize;
    if stride < 24 {
        return None;
    }
    after_runtime
        .get(8..)?
        .get(..count.checked_mul(stride)?)?
        .chunks_exact(stride)
        .map(|r| {
            Some(Binding {
                class: word(r, 0)?,
                space: word(r, 4)?,
                lower: word(r, 8)?,
                upper: word(r, 12)?,
                kind: word(r, 16)?,
            })
        })
        .collect()
}

fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// The private SRVs a translated program declares, and the title register
/// each scene stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) space: u32,
    /// Private scene registers in ascending order with the title's (space, register).
    pub(crate) scenes: Vec<(u32, (u32, u32))>,
    pub(crate) triangle_geometry: bool,
}

impl Layout {
    /// Root SRVs appended after the title's parameters, in this order.
    pub(crate) fn private_registers(&self) -> Vec<u32> {
        let mut registers: Vec<u32> = self.scenes.iter().map(|&(r, _)| r).collect();
        if self.triangle_geometry {
            registers.extend([1, 2]);
        }
        registers
    }
}

pub(crate) fn inline_layout(guest: &[Binding], host: &[Binding]) -> Option<Layout> {
    if host.iter().any(|r| r.space == SCENE_SPACE) {
        return scene_space_layout(guest, host);
    }
    let mut private = 0u8;
    for r in host.iter().filter(|r| r.space == 1) {
        let slot = r.lower;
        if !r.single() || !matches!((slot, r.kind), (0, KIND_SCENE) | (1 | 2, KIND_RAW)) {
            return None;
        }
        let bit = 1 << slot;
        if private & bit != 0 {
            return None;
        }
        private |= bit;
    }
    if !matches!(private, 1 | 7) {
        return None;
    }
    let mut scene = None;
    for r in guest {
        if r.space == 1 {
            return None;
        } // A space owned by the title stays title-owned.
        if r.scene() && (!r.single() || r.space != 0 || scene.replace(r.lower).is_some()) {
            return None;
        }
    }
    Some(Layout {
        space: 1,
        scenes: vec![(0, (0, scene?))],
        triangle_geometry: private == 7,
    })
}

fn scene_space_layout(guest: &[Binding], host: &[Binding]) -> Option<Layout> {
    if guest.iter().any(|r| r.space == SCENE_SPACE) {
        return None;
    }
    let mut titles: Vec<(u32, u32)> = Vec::new();
    for r in guest.iter().filter(|r| r.scene()) {
        if !r.single() {
            return None;
        }
        titles.push((r.space, r.lower));
    }
    titles.sort_unstable();
    if titles.windows(2).any(|w| w[0] == w[1]) {
        return None;
    }
    let mut scenes = Vec::new();
    for r in host.iter().filter(|r| r.space == SCENE_SPACE) {
        if !r.single() || !r.scene() {
            return None;
        }
        scenes.push((r.lower, *titles.get(r.lower as usize)?));
    }
    scenes.sort_unstable();
    if scenes.is_empty() || scenes.len() > MAX_SCENES || scenes.windows(2).any(|w| w[0].0 == w[1].0) {
        return None;
    }
    Some(Layout {
        space: SCENE_SPACE,
        scenes,
        triangle_geometry: false,
    })
}

// Keep the guest-provided serialization for cloning a selected PSO's layout.
// The same RTS0 can be supplied in a shader or in its own serialized container.
#[derive(Clone)]
pub(crate) struct RootSource {
    bytes: Arc<[u8]>,
    payload: Range<usize>,
    pub(crate) direct_resources: bool,
    pub(crate) paged_tables: Option<Arc<[Option<u32>]>>,
    inline_ranges: Arc<[Vec<Range<u32>>]>,
    pub(crate) placed_tables: Option<Arc<[(u32, Range<u32>)]>>,
}

impl RootSource {
    pub(crate) fn new(bytes: &[u8]) -> Self {
        let payload = tuxbox_dxbc::Dxbc::parse(bytes)
            .ok()
            .and_then(|dxbc| {
                let part = dxbc.chunk(b"RTS0")?;
                Some(part.offset..part.offset + part.size)
            })
            .unwrap_or(0..bytes.len());
        use tuxbox_dxbc::rootsig::RootParamKind;
        let root = tuxbox_dxbc::rootsig::parse_rts0(&bytes[payload.clone()]).ok();
        let placed_tables = root.as_ref().and_then(|root| root.placed_texture_tables());
        let direct_resources = root.as_ref().is_some_and(|root| root.flags & D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED.0 as u32 != 0);
        let inline_ranges = root.as_ref().map(|signature| signature.params.iter().map(|param| {
            let RootParamKind::Table(ranges) = &param.kind else { return Vec::new(); };
            let mut next = 0;
            ranges.iter().filter_map(|range| {
                let start = if range.offset_in_table == u32::MAX { next } else { range.offset_in_table };
                next = start.checked_add(range.num)?;
                (range.range_type <= 2 && range.num != u32::MAX).then_some(start..next)
            }).collect()
        }).collect::<Arc<[_]>>()).unwrap_or_default();
        let paged_tables = root
            .map(|root| root.params.iter().map(|param| {
                let RootParamKind::Table(ranges) = &param.kind else { return None };
                let mut end = 0;
                let mut extent = 0;
                for r in ranges {
                    if r.num == u32::MAX { return None; }
                    let start = if r.offset_in_table == u32::MAX { end } else { r.offset_in_table };
                    end = start.checked_add(r.num)?;
                    extent = extent.max(end);
                }
                Some(extent)
            }).collect::<Arc<[_]>>());
        Self {
            direct_resources,
            paged_tables,
            inline_ranges,
            placed_tables: placed_tables.map(Arc::from),
            bytes: Arc::from(bytes),
            payload,
        }
    }

    pub(crate) fn inline_ranges(&self, slot: usize) -> &[Range<u32>] {
        self.inline_ranges.get(slot).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.bytes, &other.bytes)
            || self.bytes[self.payload.clone()] == other.bytes[other.payload.clone()]
    }

    pub(super) fn payload(&self) -> &[u8] {
        &self.bytes[self.payload.clone()]
    }

    pub(super) fn serialized(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SceneSource {
    Table { slot: u32, offset: u32 },
    RootSrv(u32),
}

#[derive(Clone)]
pub(crate) struct SceneRoot {
    pub(crate) program: Arc<InlineProgram>,
    pub(crate) native: ID3D12RootSignature,
    pub(crate) original: RootSource,
    /// Where each title scene is bound, in private register order.
    pub(crate) sources: Arc<[SceneSource]>,
    pub(crate) appended: u32,
}

unsafe fn describe(params: &[D3D12_ROOT_PARAMETER1]) -> String {
    let mut text = String::new();
    for (i, p) in params.iter().enumerate() {
        let _ = write!(
            text,
            " p{i}(type={},vis={})",
            p.ParameterType.0, p.ShaderVisibility.0
        );
        unsafe {
            match p.ParameterType {
                D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE => {
                    let t = p.Anonymous.DescriptorTable;
                    if t.NumDescriptorRanges != 0 {
                        for r in core::slice::from_raw_parts(
                            t.pDescriptorRanges,
                            t.NumDescriptorRanges as usize,
                        ) {
                            let _ = write!(
                                text,
                                "[type={} reg={} space={} n={} off={:#x} flags={:#x}]",
                                r.RangeType.0,
                                r.BaseShaderRegister,
                                r.RegisterSpace,
                                r.NumDescriptors,
                                r.OffsetInDescriptorsFromTableStart,
                                r.Flags.0
                            );
                        }
                    }
                }
                D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS => {
                    let c = p.Anonymous.Constants;
                    let _ = write!(
                        text,
                        "[b{} space={} dwords={}]",
                        c.ShaderRegister, c.RegisterSpace, c.Num32BitValues
                    );
                }
                _ => {
                    let d = p.Anonymous.Descriptor;
                    let _ = write!(
                        text,
                        "[reg={} space={} flags={:#x}]",
                        d.ShaderRegister, d.RegisterSpace, d.Flags.0
                    );
                }
            }
        }
    }
    text
}

unsafe fn scene_sources(
    params: &[D3D12_ROOT_PARAMETER1],
    layout: &Layout,
) -> Result<Vec<SceneSource>> {
    let private = layout.private_registers();
    let mut sources: Vec<Option<SceneSource>> = vec![None; layout.scenes.len()];
    let index_of = |space: u32, register: u32| {
        layout.scenes.iter().position(|&(_, title)| title == (space, register))
    };
    let mut place = |index: usize, source: SceneSource| -> Result<()> {
        if sources[index].replace(source).is_some() {
            return Err(E_INVALIDARG.into());
        }
        Ok(())
    };
    let mut cost = 2 * private.len() as u64; // two DWORDs per root SRV
    for (slot, p) in params.iter().enumerate() {
        let slot = slot as u32;
        let visible = p.ShaderVisibility == D3D12_SHADER_VISIBILITY_ALL;
        unsafe {
            match p.ParameterType {
                D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE => {
                    cost += 1;
                    let table = p.Anonymous.DescriptorTable;
                    if !visible || table.NumDescriptorRanges == 0 {
                        continue;
                    }
                    let ranges = core::slice::from_raw_parts(
                        table.pDescriptorRanges,
                        table.NumDescriptorRanges as usize,
                    );
                    let mut next = 0u64;
                    for r in ranges {
                        let offset = if r.OffsetInDescriptorsFromTableStart
                            == D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND
                        {
                            next
                        } else {
                            u64::from(r.OffsetInDescriptorsFromTableStart)
                        };
                        next = offset.saturating_add(u64::from(r.NumDescriptors));
                        if r.RangeType != D3D12_DESCRIPTOR_RANGE_TYPE_SRV {
                            continue;
                        }
                        let end = r.BaseShaderRegister.saturating_add(r.NumDescriptors);
                        if r.RegisterSpace == layout.space
                            && private.iter().any(|&reg| (r.BaseShaderRegister..end).contains(&reg))
                        {
                            return Err(E_INVALIDARG.into());
                        }
                        for (index, &(_, (space, register))) in layout.scenes.iter().enumerate() {
                            if r.RegisterSpace == space
                                && let Some(within) = register.checked_sub(r.BaseShaderRegister)
                                && within < r.NumDescriptors
                            {
                                let offset = u32::try_from(offset + u64::from(within))
                                    .map_err(|_| E_INVALIDARG)?;
                                place(index, SceneSource::Table { slot, offset })?;
                            }
                        }
                    }
                }
                D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS => {
                    cost += u64::from(p.Anonymous.Constants.Num32BitValues)
                }
                D3D12_ROOT_PARAMETER_TYPE_CBV
                | D3D12_ROOT_PARAMETER_TYPE_UAV
                | D3D12_ROOT_PARAMETER_TYPE_SRV => {
                    cost += 2;
                    if !visible || p.ParameterType != D3D12_ROOT_PARAMETER_TYPE_SRV {
                        continue;
                    }
                    let d = p.Anonymous.Descriptor;
                    if d.RegisterSpace == layout.space && private.contains(&d.ShaderRegister) {
                        return Err(E_INVALIDARG.into());
                    }
                    if let Some(index) = index_of(d.RegisterSpace, d.ShaderRegister) {
                        place(index, SceneSource::RootSrv(slot))?;
                    }
                }
                _ => return Err(E_INVALIDARG.into()),
            }
        }
    }
    if cost > 64 {
        return Err(E_INVALIDARG.into());
    }
    sources
        .into_iter()
        .map(|source| source.ok_or_else(|| E_INVALIDARG.into()))
        .collect()
}

// Clone the root actually supplied by the caller, appending only the private
// SRVs in its private space. Use the shader's embedded root when no root was supplied.
pub(super) fn private_root(
    device: &ID3D12Device,
    source: &[u8],
    translated: &[u8],
    explicit: Option<&RootSource>,
    host_root: Option<&RootSource>,
    node_mask: u32,
) -> Result<Option<SceneRoot>> {
    let (Some(guest), Some(host)) = (bindings(source), bindings(translated)) else {
        return Ok(None);
    };
    let Some(layout) = inline_layout(&guest, &host) else {
        return Ok(None);
    };
    let program = Arc::new(InlineProgram {
        source_hash: super::shader_cache::source_digest(source).ok_or(E_INVALIDARG)?,
        triangle_geometry: layout.triangle_geometry,
        first_dispatch: Once::new(),
    });
    let private = layout.private_registers();
    let private_count = private.len() as u32;
    let original = explicit.cloned().unwrap_or_else(|| RootSource::new(source));
    // Extend the host root the program is bound with: the reflected table
    // layout when the title's root needed one, otherwise the title's root.
    // Bindings are still matched against the title's root.
    let base = host_root.cloned().unwrap_or_else(|| original.clone());
    let bytes = &base.bytes;
    let mut reader = None::<ID3D12VersionedRootSignatureDeserializer>;
    // The native deserializer owns and validates every pointer in the returned description.
    unsafe {
        D3D12CreateVersionedRootSignatureDeserializer(
            bytes.as_ptr().cast(),
            bytes.len(),
            &ID3D12VersionedRootSignatureDeserializer::IID,
            (&raw mut reader).cast(),
        )?;
        let reader = reader.ok_or(E_POINTER)?;
        let original_desc = reader
            .GetUnconvertedRootSignatureDesc()
            .as_ref()
            .ok_or(E_POINTER)?;
        let mut desc = match original_desc.Version {
            D3D_ROOT_SIGNATURE_VERSION_1_0 => {
                *reader.GetRootSignatureDescAtVersion(D3D_ROOT_SIGNATURE_VERSION_1_1)?
            }
            D3D_ROOT_SIGNATURE_VERSION_1_1 | D3D_ROOT_SIGNATURE_VERSION_1_2 => *original_desc,
            _ => return Err(E_INVALIDARG.into()),
        };
        let (count, ptr, samplers, flags) = if desc.Version == D3D_ROOT_SIGNATURE_VERSION_1_2 {
            let d = desc.Anonymous.Desc_1_2;
            (d.NumParameters, d.pParameters, d.NumStaticSamplers, d.Flags)
        } else {
            let d = desc.Anonymous.Desc_1_1;
            (d.NumParameters, d.pParameters, d.NumStaticSamplers, d.Flags)
        };
        if ptr.is_null() || count == 0 {
            return Err(E_INVALIDARG.into());
        }
        let mut params = core::slice::from_raw_parts(ptr, count as usize).to_vec();
        let binding = scene_sources(&params, &layout);
        crate::diagnostics::log_once(&Once::new(), || {
            let hash = super::shader_cache::source_digest(&bytes[base.payload.clone()])
                .unwrap_or_default();
            format!(
                "shader {} root {hash} v{} flags={:#x} staticSamplers={samplers} scene={binding:?}{}",
                program.source_hash,
                original_desc.Version.0,
                flags.0,
                describe(&params)
            )
        });
        let binding = binding?;
        for &register in &private {
            params.push(D3D12_ROOT_PARAMETER1 {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_SRV,
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
                Anonymous: D3D12_ROOT_PARAMETER1_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR1 {
                        ShaderRegister: register,
                        RegisterSpace: layout.space,
                        Flags: D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE,
                    },
                },
            });
        }
        if desc.Version == D3D_ROOT_SIGNATURE_VERSION_1_2 {
            desc.Anonymous.Desc_1_2.NumParameters = count + private_count;
            desc.Anonymous.Desc_1_2.pParameters = params.as_ptr();
        } else {
            desc.Anonymous.Desc_1_1.NumParameters = count + private_count;
            desc.Anonymous.Desc_1_1.pParameters = params.as_ptr();
        }
        let mut serialized = None;
        D3D12SerializeVersionedRootSignature(&desc, &mut serialized, None)?;
        let blob = serialized.ok_or(E_POINTER)?;
        let bytes =
            core::slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize());
        Ok(Some(SceneRoot {
            program,
            native: device.CreateRootSignature(node_mask, bytes)?,
            original,
            sources: binding.into(),
            appended: count,
        }))
    }
}

#[cfg(test)]
#[path = "../tests/device/compute_scene.rs"]
mod tests;
