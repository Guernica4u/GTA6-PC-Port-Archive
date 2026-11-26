use crate::{hlsl, ShaderExtract, Stage};

const V4: [u32; 2] = [0x7e08_02ff, 0x1357_2468];
const V5: [u32; 2] = [0x7e0a_02ff, 0x2468_1357];
const ENDPGM: u32 = 0xbf81_0000;

/// Translate `v_mov_b32 v4/v5, literal`, then `body`, then `s_endpgm`.
pub(in crate::hlsl) fn translate(stage: Stage, body: &[u32]) -> Result<String, String> {
    let words: Vec<u32> = V4.iter().chain(&V5).chain(body).chain(&[ENDPGM]).copied().collect();
    let isa: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let ex = ShaderExtract { flat_inputs: Vec::new(), flat_outputs: Vec::new(),
        stage, shader_model: (6, 0), required_features: None, bitcode_stripped: true,
        fspx_config: None, ps09_pixel_inputs: None, fssm: Vec::new(), entry: Some("main".into()),
        isa_data: isa.clone(), isa, isa_offset_in_fssm: 0, psv_threads: None, isa_preamble: [0, 1, 1, 1],
        has_input_sig: false, has_output_sig: false, has_psv0: false, has_root_sig: false,
        input_sig: Vec::new(), output_sig: Vec::new(), resources: Vec::new(), gs_info: None,
        mesh_info: None, primitive_sig: Vec::new(), root_sig: None,
    };
    hlsl::translate(&ex).map(|t| t.hlsl)
}

#[test]
fn pair_forms_write_both_destination_words() {
    // LLVM gfx1030: v_floor_f64_e32 v[8:9], v[4:5]
    let hlsl = translate(Stage::Compute, &[0x7e10_3504]).unwrap();
    assert!(hlsl.contains("tb_f64_round(uint2(v4, v5), 1u, 0xc0u)"), "{hlsl}");
    assert!(hlsl.contains("v8 = t") && hlsl.contains("v9 = t"), "{hlsl}");
    // v_rndne_f64_e64 v[8:9], v[4:5]
    assert!(translate(Stage::Compute, &[0xd599_0008, 0x0201_0104]).unwrap().contains(", 3u, 0xc0u)"));
    // v_cvt_i32_f64_e32 v8, v[4:5]
    assert!(translate(Stage::Compute, &[0x7e10_0704]).unwrap().contains("tb_f64_to_i32(uint2(v4, v5), true)"));
    // v_cvt_f64_u32_e32 v[8:9], v4
    assert!(translate(Stage::Compute, &[0x7e10_2d04]).unwrap().contains("tb_i32_to_f64(v4, false)"));
}

#[test]
fn mode_register_reaches_the_helpers() {
    // s_denorm_mode 0, then v_floor_f64_e32 v[8:9], v[4:5]
    let hlsl = translate(Stage::Compute, &[0xbfa5_0000, 0x7e10_3504]).unwrap();
    assert!(hlsl.contains("tb_f64_round(uint2(v4, v5), 1u, tb_fp_mode)"), "{hlsl}");
    // s_setreg_imm32_b32 hwreg(HW_REG_MODE, 0, 10), 0x2c0 (IEEE), then v_fract_f64_e32 v[8:9], v[4:5]
    let hlsl = translate(Stage::Compute, &[0xba80_4801, 0x0000_02c0, 0x7e10_7d04]).unwrap();
    assert!(hlsl.contains("tb_f64_fract(uint2(v4, v5), tb_fp_mode)"), "{hlsl}");
}

#[test]
fn rounding_quiets_nans_only_in_ieee_mode() {
    let hlsl = translate(Stage::Compute, &[0x7e10_3504]).unwrap();
    assert!(hlsl.contains("bool quietNaN = (fpMode & 0x200u) != 0u"), "{hlsl}");
}

/// `v_mov_b32 v6/v7, literal`: the second f64 operand.
const V6_V7: [u32; 4] = [0x7e0c_02ff, 0x3579_2468, 0x7e0e_02ff, 0x4681_3579];

fn with_v6_v7(body: &[u32]) -> Vec<u32> {
    V6_V7.iter().chain(body).copied().collect()
}

#[test]
fn conversions_frexp_and_fract() {
    for (words, call, pair) in [
        // v_cvt_f64_f32_e32 v[8:9], v4
        (&[0x7e10_2104][..], "tb_f32_to_f64(v4, 0xc0u)", true),
        // v_cvt_f32_f64_e32 v8, v[4:5]
        (&[0x7e10_1f04], "tb_f64_to_f32(uint2(v4, v5), 0xc0u)", false),
        // v_frexp_exp_i32_f64_e32 v8, v[4:5]
        (&[0x7e10_7904], "tb_f64_frexp_exp(uint2(v4, v5), 0xc0u)", false),
        // v_frexp_mant_f64_e64 v[8:9], v[4:5]
        (&[0xd5bd_0008, 0x0201_0104], "tb_f64_frexp_mant(uint2(v4, v5), 0xc0u)", true),
        // v_fract_f64_e32 v[8:9], v[4:5]
        (&[0x7e10_7d04], "tb_f64_fract(uint2(v4, v5), 0xc0u)", true),
        // v_frexp_exp_i32_f32_e32 v8, v4
        (&[0x7e10_7f04], "tb_f32_frexp_exp(v4, 0xc0u)", false),
        // v_frexp_mant_f32_e32 v8, v4
        (&[0x7e10_8104], "tb_f32_frexp_mant(v4, 0xc0u)", false),
    ] {
        let hlsl = translate(Stage::Compute, words).unwrap();
        assert!(hlsl.contains(call), "{call}\n{hlsl}");
        assert_eq!(hlsl.contains("v9 = t"), pair, "{call}\n{hlsl}");
    }
}

#[test]
fn two_source_forms_read_both_operands() {
    // v_min_f64 v[8:9], v[4:5], v[6:7]
    let hlsl = translate(Stage::Compute, &with_v6_v7(&[0xd566_0008, 0x0202_0d04])).unwrap();
    assert!(hlsl.contains("tb_f64_extrema(uint2(v4, v5), uint2(v6, v7), false, 0xc0u)"), "{hlsl}");
    // v_max_f64 v[8:9], v[4:5], v[6:7]
    let hlsl = translate(Stage::Compute, &with_v6_v7(&[0xd567_0008, 0x0202_0d04])).unwrap();
    assert!(hlsl.contains("tb_f64_extrema(uint2(v4, v5), uint2(v6, v7), true, 0xc0u)"), "{hlsl}");
    // v_ldexp_f64 v[8:9], v[4:5], v6
    let hlsl = translate(Stage::Compute, &with_v6_v7(&[0xd568_0008, 0x0202_0d04])).unwrap();
    assert!(hlsl.contains("tb_f64_ldexp(uint2(v4, v5), asint(v6), 0xc0u)"), "{hlsl}");
    // v_min_f64 v[4:5], v[4:5], v[6:7]: the result replaces its own source pair.
    let hlsl = translate(Stage::Compute, &with_v6_v7(&[0xd566_0004, 0x0202_0d04])).unwrap();
    assert!(hlsl.contains("v4 = t") && hlsl.contains("v5 = t"), "{hlsl}");
}

#[test]
fn second_pair_source_is_live_in() {
    // v_max_f64 v[8:9], v[4:5], v[6:7] without initializing v[6:7]: both
    // words of the second pair must be inputs, not undeclared locals.
    let hlsl = translate(Stage::Compute, &[0xd567_0008, 0x0202_0d04]).unwrap();
    assert!(hlsl.contains("uint2(v6, v7)"), "{hlsl}");
}

#[test]
fn modifiers_scalar_sources_and_other_stages_are_refused() {
    // v_floor_f64_e64 v[8:9], -v[4:5]
    assert!(translate(Stage::Compute, &[0xd59a_0008, 0x2201_0104]).is_err());
    // v_floor_f64_e32 v[8:9], s[4:5]
    assert!(translate(Stage::Compute, &[0x7e10_3404]).is_err());
    assert!(translate(Stage::Pixel, &[0x7e10_3504]).is_err());
    for words in [
        // v_min_f64 v[8:9], v[4:5], s[6:7]
        &[0xd566_0008, 0x0200_0d04][..],
        // v_max_f64 v[8:9], -v[4:5], v[6:7]
        &[0xd567_0008, 0x2202_0d04],
        // v_ldexp_f64 v[8:9], v[4:5], 2
        &[0xd568_0008, 0x0201_0504],
        // v_fract_f64_e64 v[8:9], v[4:5] clamp
        &[0xd5be_8008, 0x0201_0104],
        // v_cvt_f32_f64_e64 v8, v[4:5] mul:2
        &[0xd58f_0008, 0x0a01_0104],
    ] {
        assert!(translate(Stage::Compute, &with_v6_v7(words)).is_err(), "{words:x?}");
    }
    // v_fract_f64_e32 v[8:9], v[4:5]
    assert!(translate(Stage::Pixel, &[0x7e10_7d04]).is_err());
}
