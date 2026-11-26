//! Translate a synthetic compute program, supplied as native words, that
//! updates a 3D uint image through `image_atomic_cmpswap`.
//!
//! The image descriptor comes from the descriptor table in s[4:5], whose
//! single root range is the UAV u0. The thread group is 32 lanes.
use tuxbox_xshader::meta::{RootParam, RootParamKind, RootRange, RootSig};
use tuxbox_xshader::psv::PsvResource;
use tuxbox_xshader::{ShaderExtract, Stage, hlsl};

fn main() {
    let words: Vec<u32> = std::env::args().skip(1)
        .map(|s| u32::from_str_radix(&s, 16).expect("hex native word")).collect();
    assert!(!words.is_empty() && words.len() <= 256 && words.last() == Some(&0xbf810000));
    let isa: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let uav = RootRange { range_type: 1, num: 1, base_register: 0, space: 0, offset_in_table: 0 };
    let ex = ShaderExtract { flat_inputs: Vec::new(), flat_outputs: Vec::new(),
        stage: Stage::Compute, shader_model: (6, 0), required_features: None,
        bitcode_stripped: true, fspx_config: None, ps09_pixel_inputs: None,
        fssm: Vec::new(), entry: Some("main".into()), isa_data: isa.clone(),
        isa, isa_offset_in_fssm: 0, psv_threads: None, isa_preamble: [0, 32, 1, 1],
        has_input_sig: false, has_output_sig: false, has_psv0: false, has_root_sig: true,
        input_sig: Vec::new(), output_sig: Vec::new(),
        // A typed UAV at u0.
        resources: vec![PsvResource { res_type: 6, space: 0, lower: 0, upper: 0 }],
        gs_info: None, mesh_info: None, primitive_sig: Vec::new(),
        root_sig: Some(RootSig { version: 1, flags: 0, num_static_samplers: 0, static_samplers: Vec::new(),
            params: vec![RootParam { visibility: 0, kind: RootParamKind::Table(vec![uav]) }] }),
    };
    match hlsl::translate(&ex) {
        Ok(result) => print!("{}", result.hlsl),
        Err(reason) => { eprintln!("{reason}"); std::process::exit(65); }
    }
}
