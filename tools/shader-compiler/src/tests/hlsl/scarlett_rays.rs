//! ProjectScarlett's ray walks: program matching, scene numbering and the
//! declarations of the private scene space. The lowering itself is checked on
//! the GPU by tools/check_scarlett_rays.py.
use super::{declarations, identify, scene_index};
use crate::psv::PsvResource;
use crate::{ShaderExtract, Stage};

fn extract(entry: &str, isa: Vec<u8>, resources: Vec<PsvResource>) -> ShaderExtract {
    ShaderExtract {
        stage: Stage::Compute,
        shader_model: (6, 0),
        required_features: None,
        bitcode_stripped: true,
        fspx_config: None,
        ps09_pixel_inputs: None,
        fssm: Vec::new(),
        entry: Some(entry.into()),
        isa_data: isa.clone(),
        isa,
        isa_offset_in_fssm: 0,
        psv_threads: Some([32, 1, 1]),
        flat_inputs: Vec::new(),
        flat_outputs: Vec::new(),
        isa_preamble: [0, 32, 1, 1],
        has_input_sig: false,
        has_output_sig: false,
        has_psv0: true,
        has_root_sig: false,
        input_sig: Vec::new(),
        output_sig: Vec::new(),
        resources,
        gs_info: None,
        mesh_info: None,
        primitive_sig: Vec::new(),
        root_sig: None,
    }
}

fn scene(space: u32, register: u32) -> PsvResource {
    PsvResource { res_type: 4, space, lower: register, upper: register }
}

#[test]
fn only_the_fingerprinted_programs_are_lowered() {
    // The entry name alone is not enough: another build of the program has
    // different registers and offsets.
    let ex = extract("CS_IrradianceCache_ShadowTrace_ad5ad774_Wrapped", vec![0; 2832], Vec::new());
    assert_eq!(identify(&ex), None);
    let mut other_stage = ex.clone();
    other_stage.stage = Stage::Pixel;
    assert_eq!(identify(&other_stage), None);
    let shadows = extract("CS_RaytraceShadows_Mixed_22f3acbc_Wrapped", vec![0; 7728], Vec::new());
    assert_eq!(identify(&shadows), None);
    let laser = extract("CS_LaserLightShadowRaytracing_c3ab3042_Wrapped", vec![0; 3332], Vec::new());
    assert_eq!(identify(&laser), None);
}

#[test]
fn scenes_are_numbered_by_space_then_register() {
    let ex = extract("main", Vec::new(), vec![
        scene(1, 356),
        PsvResource { res_type: 3, space: 1, lower: 339, upper: 339 },
        scene(1, 355),
        scene(0, 5),
        PsvResource { res_type: 4, space: 2, lower: 0, upper: 3 },
    ]);
    assert_eq!(scene_index(&ex, 0, 5), Some(0));
    assert_eq!(scene_index(&ex, 1, 355), Some(1));
    assert_eq!(scene_index(&ex, 1, 356), Some(2));
    // A typed SRV and a range of structures have no private scene.
    assert_eq!(scene_index(&ex, 1, 339), None);
    assert_eq!(scene_index(&ex, 2, 0), None);
}

#[test]
fn only_scenes_the_program_reads_are_declared() {
    let hlsl = "v7 = tbScarlettOccluded(tbScarlettScene1, a, b, 0.0, c) ? 1u : 0u;";
    let declared = declarations(hlsl);
    assert!(declared.contains("RaytracingAccelerationStructure tbScarlettScene1 : register(t1, space900);"), "{declared}");
    assert!(!declared.contains("tbScarlettScene0 :"), "{declared}");
    assert!(declared.contains("bool tbScarlettOccluded("), "{declared}");
}
