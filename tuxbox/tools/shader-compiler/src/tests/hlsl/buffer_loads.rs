//! Loads from constant, raw and typed buffers, and stores and atomics to writable
//! ones, for `tools/check_buffer_loads.py`.
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
fn program(range_type: u32, view: Option<u32>, threads: u32, words: &[u32]) -> ShaderExtract {
    let isa: Vec<u8> = [&DESCRIPTOR[..], words, &[END]].concat().iter().flat_map(|w| w.to_le_bytes()).collect();
    let range = RootRange { range_type, num: 4, base_register: 0, space: 0, offset_in_table: 0 };
    ShaderExtract {
        flat_inputs: Vec::new(), flat_outputs: Vec::new(), stage: Stage::Compute, shader_model: (6, 0),
        required_features: None, bitcode_stripped: true, fspx_config: None, ps09_pixel_inputs: None,
        fssm: Vec::new(), entry: Some("main".into()), isa_data: isa.clone(), isa, isa_offset_in_fssm: 0,
        psv_threads: None, isa_preamble: [0, threads, 1, 1], has_input_sig: false, has_output_sig: false,
        has_psv0: false, has_root_sig: true, input_sig: Vec::new(), output_sig: Vec::new(),
        resources: view.map(|res_type| PsvResource { res_type, space: 0, lower: 1, upper: 1 }).into_iter().collect(),
        gs_info: None, mesh_info: None, primitive_sig: Vec::new(),
        root_sig: Some(RootSig { version: 1, flags: 0, num_static_samplers: 0, static_samplers: Vec::new(),
            params: vec![RootParam { visibility: 0, kind: RootParamKind::Table(vec![range]) }] }),
    }
}

/// One program: its name, the buffer kind the check binds, the root range type
/// and PSV view type of slot 1, the registers holding the result, what they
/// must hold (`pN` is word N of the source buffer, anything else a literal),
/// and the instructions after the descriptor load.
type Case = (&'static str, &'static str, u32, Option<u32>, &'static str, &'static str, &'static [u32]);

const A: u32 = 0x1122_3344;
const B: u32 = 0x5566_7788;
/// `v_mov_b32 v5..v8, literal`: the data of the stores and atomics.
const DATA: [u32; 8] = [0x7e0a_02ff, A, 0x7e0c_02ff, B, 0x7e0e_02ff, 0x0bad_f00d, 0x7e10_02ff, 0x1234_5678];
/// `buffer_load_dwordx4 v[9:12], v4, s[8:11], 0 offen` and `buffer_load_dword v9, ...`: what a write left.
const BACK4: [u32; 2] = [0xe038_1000, 0x8002_0904];
const BACK1: [u32; 2] = [0xe030_1000, 0x8002_0904];
/// `buffer_load_format_xyzw v[9:12], v4, s[8:11], 0 idxen`.
const BACK_TYPED: [u32; 2] = [0xe00c_2000, 0x8002_0904];

macro_rules! words {
    ($($part:expr),+) => {{
        const PARTS: &[&[u32]] = &[$(&$part),+];
        const LEN: usize = { let mut n = 0; let mut i = 0; while i < PARTS.len() { n += PARTS[i].len(); i += 1; } n };
        const JOINED: [u32; LEN] = {
            let mut out = [0u32; LEN];
            let (mut at, mut i) = (0, 0);
            while i < PARTS.len() { let mut j = 0; while j < PARTS[i].len() { out[at] = PARTS[i][j]; at += 1; j += 1; } i += 1; }
            out
        };
        &JOINED
    }};
}

const CASES: &[Case] = &[
    // s_buffer_load_dword* s20.., s[8:11], 0x10
    ("constant-scalar-x1", "constant", 2, None, "s20", "p4", &[0xf420_0504, 0xfa00_0010]),
    ("constant-scalar-x2", "constant", 2, None, "s20 s21", "p4 p5", &[0xf424_0504, 0xfa00_0010]),
    ("constant-scalar-x4", "constant", 2, None, "s20 s21 s22 s23", "p4 p5 p6 p7", &[0xf428_0504, 0xfa00_0010]),
    ("constant-scalar-x8", "constant", 2, None, "s20 s21 s22 s23 s24 s25 s26 s27", "p4 p5 p6 p7 p8 p9 p10 p11",
     &[0xf42c_0504, 0xfa00_0010]),
    ("constant-scalar-x16", "constant", 2, None,
     "s20 s21 s22 s23 s24 s25 s26 s27 s28 s29 s30 s31 s32 s33 s34 s35",
     "p4 p5 p6 p7 p8 p9 p10 p11 p12 p13 p14 p15 p16 p17 p18 p19", &[0xf430_0504, 0xfa00_0010]),
    ("raw-scalar-x4", "raw", 0, Some(4), "s20 s21 s22 s23", "p4 p5 p6 p7", &[0xf428_0504, 0xfa00_0010]),
    // buffer_load_dword v7, off, s[8:11], 0 offset:8
    ("raw-vector-fixed", "raw", 0, Some(4), "v7", "p2", &[0xe030_0008, 0x8002_0700]),
    // buffer_load_dword* v7.., v4, s[8:11], 0 offen
    ("raw-vector-x1", "raw", 0, Some(4), "v7", "p4", &[SIXTEEN, 0xe030_1000, 0x8002_0704]),
    ("raw-vector-x2", "raw", 0, Some(4), "v7 v8", "p4 p5", &[SIXTEEN, 0xe034_1000, 0x8002_0704]),
    ("raw-vector-x3", "raw", 0, Some(4), "v7 v8 v9", "p4 p5 p6", &[SIXTEEN, 0xe03c_1000, 0x8002_0704]),
    ("raw-vector-x4", "raw", 0, Some(4), "v7 v8 v9 v10", "p4 p5 p6 p7", &[SIXTEEN, 0xe038_1000, 0x8002_0704]),
    ("constant-vector-x4", "constant", 2, None, "v7 v8 v9 v10", "p4 p5 p6 p7", &[SIXTEEN, 0xe038_1000, 0x8002_0704]),
    // buffer_load_format_* v7.., v4, s[8:11], 0 idxen: element 16 of four-word elements
    ("typed-x", "typed", 0, Some(3), "v7", "p64", &[SIXTEEN, 0xe000_2000, 0x8002_0704]),
    ("typed-xy", "typed", 0, Some(3), "v7 v8", "p64 p65", &[SIXTEEN, 0xe004_2000, 0x8002_0704]),
    ("typed-xyz", "typed", 0, Some(3), "v7 v8 v9", "p64 p65 p66", &[SIXTEEN, 0xe008_2000, 0x8002_0704]),
    ("typed-xyzw", "typed", 0, Some(3), "v7 v8 v9 v10", "p64 p65 p66 p67", &[SIXTEEN, 0xe00c_2000, 0x8002_0704]),
    // buffer_store_dword* v5.., v4, s[8:11], 0 offen, then the four words read back
    ("store-x1", "raw-write", 1, Some(7), "v9 v10 v11 v12", "0x11223344 p5 p6 p7",
     words!([SIXTEEN], DATA, [0xe070_1000, 0x8002_0504], BACK4)),
    ("store-x2", "raw-write", 1, Some(7), "v9 v10 v11 v12", "0x11223344 0x55667788 p6 p7",
     words!([SIXTEEN], DATA, [0xe074_1000, 0x8002_0504], BACK4)),
    ("store-x3", "raw-write", 1, Some(7), "v9 v10 v11 v12", "0x11223344 0x55667788 0x0badf00d p7",
     words!([SIXTEEN], DATA, [0xe07c_1000, 0x8002_0504], BACK4)),
    ("store-x4", "raw-write", 1, Some(7), "v9 v10 v11 v12", "0x11223344 0x55667788 0x0badf00d 0x12345678",
     words!([SIXTEEN], DATA, [0xe078_1000, 0x8002_0504], BACK4)),
    // buffer_store_format_xyzw v[5:8], v4, s[8:11], 0 idxen
    ("store-typed-xyzw", "typed-write", 1, Some(6), "v9 v10 v11 v12", "0x11223344 0x55667788 0x0badf00d 0x12345678",
     words!([SIXTEEN], DATA, [0xe01c_2000, 0x8002_0504], BACK_TYPED)),
    // buffer_atomic_* v5, v4, s[8:11], 0 offen on word 4, which holds 0x41000404
    ("atomic-add", "raw-write", 1, Some(7), "v9", "0x52223748", words!([SIXTEEN], DATA, [0xe0c8_1000, 0x8002_0504], BACK1)),
    // With GLC the data register receives the word as it was.
    ("atomic-add-return", "raw-write", 1, Some(7), "v9 v5", "0x52223748 p4",
     words!([SIXTEEN], DATA, [0xe0c8_5000, 0x8002_0504], BACK1)),
    ("atomic-umin", "raw-write", 1, Some(7), "v9", "0x11223344", words!([SIXTEEN], DATA, [0xe0d8_1000, 0x8002_0504], BACK1)),
    ("atomic-umax", "raw-write", 1, Some(7), "v9", "0x55667788",
     words!([SIXTEEN], [0x7e0a_02ff, B], [0xe0e0_1000, 0x8002_0504], BACK1)),
    ("atomic-smin", "raw-write", 1, Some(7), "v9", "0xf0000001",
     words!([SIXTEEN], [0x7e0a_02ff, 0xf000_0001], [0xe0d4_1000, 0x8002_0504], BACK1)),
    ("atomic-smax", "raw-write", 1, Some(7), "v9", "0x55667788",
     words!([SIXTEEN], [0x7e0a_02ff, B], [0xe0dc_1000, 0x8002_0504], BACK1)),
    // A negative operand is below a positive word when signed, and above it when unsigned.
    ("atomic-smax-negative", "raw-write", 1, Some(7), "v9", "p4",
     words!([SIXTEEN], [0x7e0a_02ff, 0xf000_0001], [0xe0dc_1000, 0x8002_0504], BACK1)),
    ("atomic-umax-high", "raw-write", 1, Some(7), "v9", "0xf0000001",
     words!([SIXTEEN], [0x7e0a_02ff, 0xf000_0001], [0xe0e0_1000, 0x8002_0504], BACK1)),
    ("atomic-or", "raw-write", 1, Some(7), "v9", "0x51223744", words!([SIXTEEN], DATA, [0xe0e8_1000, 0x8002_0504], BACK1)),
    ("atomic-and", "raw-write", 1, Some(7), "v9", "0x01000004", words!([SIXTEEN], DATA, [0xe0e4_1000, 0x8002_0504], BACK1)),
    ("atomic-swap", "raw-write", 1, Some(7), "v9", "0x11223344", words!([SIXTEEN], DATA, [0xe0c0_1000, 0x8002_0504], BACK1)),
    // buffer_atomic_cmpswap v[5:6]: v5 is stored where the word equals v6.
    ("atomic-cmpswap-equal", "raw-write", 1, Some(7), "v9", "0x11223344",
     words!([SIXTEEN], [0x7e0a_02ff, A, 0x7e0c_02ff, 0x4100_0404], [0xe0c4_1000, 0x8002_0504], BACK1)),
    ("atomic-cmpswap-differs", "raw-write", 1, Some(7), "v9", "p4",
     words!([SIXTEEN], DATA, [0xe0c4_1000, 0x8002_0504], BACK1)),
];

/// The programs `tools/check_buffer_loads.py` runs on the GPU.
#[test]
fn buffer_loads_for_the_gpu_check() {
    let mut text = String::new();
    for &(name, kind, range_type, view, results, expected, words) in CASES {
        // A write is checked by reading it back, so one thread runs it.
        let threads = if kind.ends_with("-write") { 1 } else { 32 };
        let hlsl = super::translate(&program(range_type, view, threads, words)).unwrap_or_else(|why| panic!("{name}: {why}")).hlsl;
        let declared = match kind {
            "constant" => "cbuffer CB1 : register(b1)",
            "raw" => "ByteAddressBuffer raw_sbuf1 : register(t1);",
            "typed" => "Buffer<float4> stb1 : register(t1);",
            "raw-write" => "RWByteAddressBuffer raw_ubuf1 : register(u1);",
            _ => "RWBuffer<float4> utb1 : register(u1);",
        };
        assert_eq!(results.split(' ').count(), expected.split(' ').count(), "{name}");
        assert!(hlsl.contains(declared), "{name}: {hlsl}");
        for register in results.split(' ') {
            assert!(hlsl.contains(&format!("{register} = ")), "{name} never writes {register}: {hlsl}");
        }
        text.push_str(&format!("//// {name} {kind} {results} | {expected}\n{hlsl}\n"));
    }
    println!("BEGIN_HLSL\n{text}END_HLSL");
}
