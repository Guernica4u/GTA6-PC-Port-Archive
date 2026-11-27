//! Loads from constant, raw and typed buffers, for `tools/check_buffer_loads.py`.
//! Encodings are from llvm-mc `-mcpu=gfx1030`.
use crate::meta::{RootParam, RootParamKind, RootRange, RootSig};
use crate::psv::PsvResource;
use crate::{ShaderExtract, Stage};

const END: u32 = 0xbf81_0000;
/// `s_load_dwordx4 s[8:11], s[4:5], 0x20`: the descriptor of slot 1 of the first table.
const DESCRIPTOR: [u32; 2] = [0xf408_0202, 0xfa00_0020];
/// `v_mov_b32 v4, 16`: the byte offset or element index of the vector loads.
const SIXTEEN: u32 = 0x7e08_0290;

/// A compute program under one table of four descriptors of `range_type`, with
/// PSV naming slot 1 a resource of `view` where the root alone does not say.
fn program(range_type: u32, view: Option<u32>, words: &[u32]) -> ShaderExtract {
    let isa: Vec<u8> = [&DESCRIPTOR[..], words, &[END]].concat().iter().flat_map(|w| w.to_le_bytes()).collect();
    let range = RootRange { range_type, num: 4, base_register: 0, space: 0, offset_in_table: 0 };
    ShaderExtract {
        flat_inputs: Vec::new(), flat_outputs: Vec::new(), stage: Stage::Compute, shader_model: (6, 0),
        required_features: None, bitcode_stripped: true, fspx_config: None, ps09_pixel_inputs: None,
        fssm: Vec::new(), entry: Some("main".into()), isa_data: isa.clone(), isa, isa_offset_in_fssm: 0,
        psv_threads: None, isa_preamble: [0, 32, 1, 1], has_input_sig: false, has_output_sig: false,
        has_psv0: false, has_root_sig: true, input_sig: Vec::new(), output_sig: Vec::new(),
        resources: view.map(|res_type| PsvResource { res_type, space: 0, lower: 1, upper: 1 }).into_iter().collect(),
        gs_info: None, mesh_info: None, primitive_sig: Vec::new(),
        root_sig: Some(RootSig { version: 1, flags: 0, num_static_samplers: 0, static_samplers: Vec::new(),
            params: vec![RootParam { visibility: 0, kind: RootParamKind::Table(vec![range]) }] }),
    }
}

/// name, buffer kind, root range type, PSV view type, the registers that hold
/// the result, the first word read, and the instructions after the descriptor load.
type Case = (&'static str, &'static str, u32, Option<u32>, &'static str, u32, &'static [u32]);

const CASES: &[Case] = &[
    // s_buffer_load_dword* s20.., s[8:11], 0x10
    ("constant-scalar-x1", "constant", 2, None, "s20", 4, &[0xf420_0504, 0xfa00_0010]),
    ("constant-scalar-x2", "constant", 2, None, "s20 s21", 4, &[0xf424_0504, 0xfa00_0010]),
    ("constant-scalar-x4", "constant", 2, None, "s20 s21 s22 s23", 4, &[0xf428_0504, 0xfa00_0010]),
    ("constant-scalar-x8", "constant", 2, None, "s20 s21 s22 s23 s24 s25 s26 s27", 4, &[0xf42c_0504, 0xfa00_0010]),
    ("constant-scalar-x16", "constant", 2, None,
     "s20 s21 s22 s23 s24 s25 s26 s27 s28 s29 s30 s31 s32 s33 s34 s35", 4, &[0xf430_0504, 0xfa00_0010]),
    ("raw-scalar-x4", "raw", 0, Some(4), "s20 s21 s22 s23", 4, &[0xf428_0504, 0xfa00_0010]),
    // buffer_load_dword v7, off, s[8:11], 0 offset:8
    ("raw-vector-fixed", "raw", 0, Some(4), "v7", 2, &[0xe030_0008, 0x8002_0700]),
    // buffer_load_dword* v7.., v4, s[8:11], 0 offen
    ("raw-vector-x1", "raw", 0, Some(4), "v7", 4, &[SIXTEEN, 0xe030_1000, 0x8002_0704]),
    ("raw-vector-x2", "raw", 0, Some(4), "v7 v8", 4, &[SIXTEEN, 0xe034_1000, 0x8002_0704]),
    ("raw-vector-x3", "raw", 0, Some(4), "v7 v8 v9", 4, &[SIXTEEN, 0xe03c_1000, 0x8002_0704]),
    ("raw-vector-x4", "raw", 0, Some(4), "v7 v8 v9 v10", 4, &[SIXTEEN, 0xe038_1000, 0x8002_0704]),
    ("constant-vector-x4", "constant", 2, None, "v7 v8 v9 v10", 4, &[SIXTEEN, 0xe038_1000, 0x8002_0704]),
    // buffer_load_format_* v7.., v4, s[8:11], 0 idxen: element 16 of four-word elements
    ("typed-x", "typed", 0, Some(3), "v7", 64, &[SIXTEEN, 0xe000_2000, 0x8002_0704]),
    ("typed-xy", "typed", 0, Some(3), "v7 v8", 64, &[SIXTEEN, 0xe004_2000, 0x8002_0704]),
    ("typed-xyz", "typed", 0, Some(3), "v7 v8 v9", 64, &[SIXTEEN, 0xe008_2000, 0x8002_0704]),
    ("typed-xyzw", "typed", 0, Some(3), "v7 v8 v9 v10", 64, &[SIXTEEN, 0xe00c_2000, 0x8002_0704]),
];

/// The programs `tools/check_buffer_loads.py` runs on the GPU.
#[test]
fn buffer_loads_for_the_gpu_check() {
    let mut text = String::new();
    for &(name, kind, range_type, view, results, first, words) in CASES {
        let hlsl = super::translate(&program(range_type, view, words)).unwrap_or_else(|why| panic!("{name}: {why}")).hlsl;
        let declared = match kind {
            "constant" => "cbuffer CB1 : register(b1)",
            "raw" => "ByteAddressBuffer raw_sbuf1 : register(t1);",
            _ => "Buffer<float4> stb1 : register(t1);",
        };
        assert!(hlsl.contains(declared), "{name}: {hlsl}");
        for register in results.split(' ') {
            assert!(hlsl.contains(&format!("{register} = ")), "{name} never writes {register}: {hlsl}");
        }
        text.push_str(&format!("//// {name} {kind} {first} {results}\n{hlsl}\n"));
    }
    println!("BEGIN_HLSL\n{text}END_HLSL");
}
