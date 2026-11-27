use crate::hlsl::float64::tests::translate;
use crate::Stage;

// LLVM gfx1030 encodings; the test prologue writes v4 and v5.
const FORMS: [(&[u32], &str); 7] = [
    (&[0x7e10_b304], "tb_f16_frexp_mant(v4, 0xc0u)"),           // v_frexp_mant_f16_e32 v8, v4
    (&[0x7e10_b504], "tb_f16_frexp_exp(v4, 0xc0u)"),            // v_frexp_exp_i16_f16_e32 v8, v4
    (&[0x7610_0b04], "tb_f16_ldexp(v4, v5, 0xc0u)"),            // v_ldexp_f16_e32 v8, v4, v5
    (&[0xd53b_0008, 0x0202_0b04], "tb_f16_ldexp(v4, v5, 0xc0u)"), // v_ldexp_f16_e64 v8, v4, v5
    (&[0xd712_0008, 0x0202_0b04], "(tb_norm_f16(v4, 0xc0u, true) | (tb_norm_f16(v5, 0xc0u, true) << 16))"),
    (&[0xd713_0008, 0x0202_0b04], "(tb_norm_f16(v4, 0xc0u, false) | (tb_norm_f16(v5, 0xc0u, false) << 16))"),
    (&[0xd768_0008, 0x0202_0b04], "(tb_norm_f32(v4, true) | (tb_norm_f32(v5, true) << 16))"),
];

fn entry_live(hlsl: &str) -> &str {
    hlsl.lines().find(|l| l.starts_with("// entry-live registers")).map_or("", |l| l)
}

#[test]
fn frexp_and_ldexp_keep_the_high_half_and_packs_replace_the_word() {
    for (k, (body, call)) in FORMS.iter().enumerate() {
        let hlsl = translate(Stage::Compute, body).unwrap_or_else(|e| panic!("{body:x?}: {e}"));
        let keeps = k < 4;
        let write = if keeps { format!("v8 = ((v8 & 0xffff0000u) | {call});") } else { format!("v8 = {call};") };
        assert!(hlsl.contains(&write), "{hlsl}");
        // A kept half makes the old destination an entry-live register.
        assert_eq!(entry_live(&hlsl).ends_with(" v8"), keeps, "{}", entry_live(&hlsl));
    }
}

#[test]
fn exact_half_forms_follow_a_written_mode() {
    // s_setreg_imm32_b32 hwreg(HW_REG_MODE, 0, 10), 0x2cf; v_ldexp_f16_e32 v8, v4, v5
    let hlsl = translate(Stage::Compute, &[0xba80_4801, 0x2cf, 0x7610_0b04]).unwrap();
    assert!(hlsl.contains("tb_f16_ldexp(v4, v5, tb_fp_mode)"), "{hlsl}");
}

#[test]
fn exact_half_forms_refuse_modifiers_scalars_constants_and_other_stages() {
    for body in [
        &[0xd5d9_0008, 0x2201_0104][..], // v_frexp_mant_f16_e64 v8, -v4
        &[0xd53b_8008, 0x0202_0b04],     // v_ldexp_f16_e64 v8, v4, v5 clamp
        &[0xd713_0808, 0x0202_0b04],     // v_cvt_pknorm_u16_f16 v8, v4, v5 op_sel:[1,0,0]
        &[0xd768_0008, 0x0202_0a04],     // v_cvt_pknorm_i16_f32 v8, s4, v5
        &[0x7e10_b2f2],                  // v_frexp_mant_f16_e32 v8, 1.0
    ] {
        assert!(translate(Stage::Compute, body).is_err(), "{body:x?}");
    }
    assert!(translate(Stage::Pixel, &[0x7610_0b04]).is_err());
}

#[test]
fn unorm16_pack_of_f32_rounds_the_exact_product() {
    // v_cvt_pknorm_u16_f32 v8, |v4|, -v5
    let hlsl = translate(Stage::Compute, &[0xd769_0108, 0x4202_0b04]).unwrap();
    assert!(hlsl.contains("(tb_norm_f32((asuint(asfloat(v4)) & 0x7fffffffu), false) | (tb_norm_f32((asuint(asfloat(v5)) ^ 0x80000000u), false) << 16))"), "{hlsl}");
    assert!(hlsl.contains("uint tb_norm_f32("), "{hlsl}");
}
