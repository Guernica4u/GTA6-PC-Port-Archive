//! 32-bit `image_atomic_cmpswap` on a typed UAV.
//!
//! DMASK 0x3 selects two data VGPRs: DATA[0] is the value stored and DATA[1]
//! the value compared. With GLC the texel's previous value returns in DATA[0].
//! HLSL's `InterlockedCompareExchange(dest, compare, value, original)` takes
//! them in the other order. Encodings are from llvm-mc `-mcpu=gfx1030`.

use super::{Emitter, Res};
use crate::rdna2::disassemble;
use crate::{ShaderExtract, Stage};

fn compute_extract(words: &[u32]) -> ShaderExtract {
    let isa: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    ShaderExtract { flat_inputs: Vec::new(), flat_outputs: Vec::new(),
        stage: Stage::Compute,
        shader_model: (6, 0),
        required_features: None,
        bitcode_stripped: true,
        fspx_config: None,
        ps09_pixel_inputs: None,
        fssm: Vec::new(),
        entry: Some("main".into()),
        isa_data: isa.clone(),
        isa,
        isa_offset_in_fssm: 0,
        psv_threads: None,
        isa_preamble: [0, 64, 1, 1],
        has_input_sig: false,
        has_output_sig: false,
        has_psv0: false,
        has_root_sig: false,
        input_sig: Vec::new(),
        output_sig: Vec::new(),
        resources: Vec::new(),
        gs_info: None,
        mesh_info: None,
        primitive_sig: Vec::new(),
        root_sig: None,
    }
}

/// Emit the first instruction against UAV u0 of dimension `dim`, typed uint as
/// `resolve_resources` records for an image atomic.
fn emit(words: &[u32], dim: u32) -> Result<String, String> {
    let ex = compute_extract(words);
    let mut em = Emitter::new(&ex, disassemble(&ex.isa));
    em.res.insert(0, vec![Res::Uav(0, dim)]);
    em.uav.insert(0, dim);
    em.uav_uint.insert(0);
    let op = em.insns[0].clone();
    assert_eq!(op.mnemonic, "image_atomic_cmpswap");
    em.emit_mimg(0, &op)?;
    Ok(em.body)
}

const END: u32 = 0xbf810000;

#[test]
/// image_atomic_cmpswap v[2:3], v[0:1], s[0:7] dmask:0x3 dim:SQ_RSRC_IMG_2D unorm glc
fn returning_2d_compare_swap() {
    let body = emit(&[0xf0403308, 0x00000200, END], 1).unwrap();
    assert!(body.contains("if (exec_lo & 1u) InterlockedCompareExchange(uav0[uint2(v0, v1)], v3, v2, t0);"), "{body}");
    assert!(body.contains("v2 = t0;"), "{body}");
    assert!(!body.contains("v3 = t0"), "{body}");
}

#[test]
/// Without GLC nothing is returned and DATA[0] keeps the value stored.
fn non_returning_compare_swap() {
    let body = emit(&[0xf0401308, 0x00000200, END], 1).unwrap();
    assert!(body.contains("InterlockedCompareExchange(uav0[uint2(v0, v1)], v3, v2, t0);"), "{body}");
    assert!(!body.contains("v2 = t0"), "{body}");
}

#[test]
/// image_atomic_cmpswap v[18:19], [v13, v14, v20], s[8:15] dmask:0x3 dim:SQ_RSRC_IMG_3D unorm glc
/// The non-sequential (NSA) 3D form ProjectScarlett's voxel boundary pass uses in a CAS loop.
fn returning_3d_nsa_compare_swap() {
    let body = emit(&[0xf0403312, 0x0002120d, 0x0000140e, END], 2).unwrap();
    assert!(body.contains("InterlockedCompareExchange(uav0[uint3(v13, v14, v20)], v19, v18, t0);"), "{body}");
    assert!(body.contains("v18 = t0;"), "{body}");
}

#[test]
/// DMASK 0xf is the 64-bit compare-swap, a multisampled texel needs its sample
/// index, and only a uint UAV has an atomic. Each says why it is refused.
fn other_compare_swap_forms_stay_refused() {
    // image_atomic_cmpswap v[2:5], v[0:1], s[0:7] dmask:0xf dim:SQ_RSRC_IMG_2D unorm glc
    assert_eq!(emit(&[0xf0403f08, 0x00000200, END], 1).unwrap_err(), "64-bit image_atomic_cmpswap (DMASK 0xf) unsupported");
    assert_eq!(emit(&[0xf0403308, 0x00000200, END], 6).unwrap_err(), "image_atomic_cmpswap on a multisampled image unsupported");
    let ex = compute_extract(&[0xf0403308, 0x00000200, END]);
    let mut em = Emitter::new(&ex, disassemble(&ex.isa));
    em.res.insert(0, vec![Res::Uav(0, 1)]);
    em.uav.insert(0, 1);
    let op = em.insns[0].clone();
    assert_eq!(em.emit_mimg(0, &op).unwrap_err(), "image_atomic_cmpswap on a UAV not typed uint");
    let mut em = Emitter::new(&ex, disassemble(&ex.isa));
    em.res.insert(0, vec![Res::Tex(0, 1)]);
    em.tex.insert(0, 1);
    assert_eq!(em.emit_mimg(0, &op).unwrap_err(), "image atomic on a non-UAV table");
}

/// Load the image descriptor from the table in s[4:5], whose root range is the
/// UAV u0, then run `op`. Encodings from llvm-mc `-mcpu=gfx1030`.
fn table_extract(op: [u32; 2]) -> ShaderExtract {
    use crate::meta::{RootParam, RootParamKind, RootRange, RootSig};
    // s_load_dwordx8 s[8:15], s[4:5], 0x0; s_waitcnt lgkmcnt(0)
    let mut ex = compute_extract(&[0xf40c0202, 0xfa000000, 0xbf8cc07f, op[0], op[1], END]);
    ex.resources.push(crate::psv::PsvResource { res_type: 6, space: 0, lower: 0, upper: 0 });
    ex.root_sig = Some(RootSig { version: 1, flags: 0, num_static_samplers: 0, static_samplers: Vec::new(),
        params: vec![RootParam { visibility: 0, kind: RootParamKind::Table(vec![
            RootRange { range_type: 1, num: 1, base_register: 0, space: 0, offset_in_table: 0 },
        ]) }] });
    ex
}

#[test]
/// `resolve_resources` types the UAV a compare-swap uses as uint, which the
/// emitter requires. A texel load alone leaves the view's type open.
fn compare_swap_types_its_uav_uint() {
    // image_atomic_cmpswap v[2:3], v[0:1], s[8:15] dmask:0x3 dim:SQ_RSRC_IMG_2D unorm glc
    let ex = table_extract([0xf0403308, 0x00020200]);
    let mut em = Emitter::new(&ex, disassemble(&ex.isa));
    em.resolve_resources().unwrap();
    assert_eq!(em.res.get(&2), Some(&vec![Res::Uav(0, 1)]));
    assert!(em.uav_uint.contains(&0));
    let hlsl = super::translate(&ex).unwrap().hlsl;
    assert!(hlsl.contains("RWTexture2D<uint> uav0 : register(u0);"), "{hlsl}");
    assert!(hlsl.contains("InterlockedCompareExchange(uav0[uint2(v0, v1)], v3, v2, t0);"), "{hlsl}");

    // image_load v2, v[0:1], s[8:15] dmask:0x1 dim:SQ_RSRC_IMG_2D unorm
    let ex = table_extract([0xf0001108, 0x00020200]);
    let mut em = Emitter::new(&ex, disassemble(&ex.isa));
    em.resolve_resources().unwrap();
    assert_eq!(em.res.get(&2), Some(&vec![Res::Uav(0, 1)]));
    assert!(!em.uav_uint.contains(&0));
}
