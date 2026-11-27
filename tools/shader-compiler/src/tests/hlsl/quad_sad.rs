use crate::hlsl::float64::tests::translate;
use crate::Stage;

// LLVM gfx1030 encodings.
const QSAD: [u32; 2] = [0xd572_0000, 0x041a_0902]; // v_qsad_pk_u16_u8 v[0:1], v[2:3], v4, v[6:7]
const MQSAD_PK_ON_S0: [u32; 2] = [0xd573_0000, 0x041a_0900]; // v_mqsad_pk_u16_u8 v[0:1], v[0:1], v4, v[6:7]
const MQSAD_ON_ACC: [u32; 2] = [0xd575_0008, 0x0422_0902]; // v_mqsad_u32_u8 v[8:11], v[2:3], v4, v[8:11]

fn entry_live(hlsl: &str) -> &str {
    hlsl.lines().find(|l| l.starts_with("// entry-live registers")).unwrap()
}

#[test]
fn quad_sads_read_every_source_word_and_write_every_result_word() {
    let hlsl = translate(Stage::Compute, &QSAD).unwrap();
    assert!(hlsl.contains("((v2 >> 24u) | (v3 << 8u))"), "{hlsl}");
    assert!(entry_live(&hlsl).ends_with(" v2 v3 v6 v7"), "{hlsl}");
    assert!(hlsl.contains("v0 = t0r0;") && hlsl.contains("v1 = t0r1;"), "{hlsl}");
    // Unmasked: no byte of S1 is tested against zero.
    assert!(!hlsl.contains("!= 0u ?"), "{hlsl}");

    let hlsl = translate(Stage::Compute, &MQSAD_ON_ACC).unwrap();
    // The test prologue writes v4 and v5.
    assert!(entry_live(&hlsl).ends_with(" v2 v3 v8 v9 v10 v11"), "{hlsl}");
    assert_eq!(hlsl.matches("((v4 >> 24u) & 0xffu) != 0u ?").count(), 4, "{hlsl}");
    // All four sums use the old accumulator before any destination word is written.
    let first_write = hlsl.find("v8 = t0r0;").unwrap();
    assert!(hlsl.find("uint t0r3 = ").unwrap() < first_write, "{hlsl}");
    assert!(hlsl.contains("v11 = t0r3;"), "{hlsl}");
}

#[test]
fn quad_sad_windows_are_taken_before_s0_is_overwritten() {
    let hlsl = translate(Stage::Compute, &MQSAD_PK_ON_S0).unwrap();
    let first_write = hlsl.find("v0 = t0r0;").unwrap();
    assert!(hlsl.find("uint t0w3 = ((v0 >> 24u) | (v1 << 8u));").unwrap() < first_write, "{hlsl}");
    assert!(hlsl.find("uint t0r1 = ").unwrap() < first_write, "{hlsl}");
}

#[test]
fn quad_sads_refuse_modifiers_scalars_overflow_and_other_stages() {
    for body in [
        [QSAD[0] | 0x8000, QSAD[1]],         // clamp
        [QSAD[0] | 0x0800, QSAD[1]],         // op_sel
        [QSAD[0], QSAD[1] | 1 << 29],        // neg
        [0xd575_0000, 0x0422_0802],          // v_mqsad_u32_u8 v[0:3], s[2:3], v4, v[8:11]
        [0xd572_00ff, 0x041a_0902],          // destination pair from v255
        [0xd575_0000, 0x07f6_0902],          // accumulator from v253
    ] {
        assert!(translate(Stage::Compute, &body).is_err(), "{body:x?}");
    }
    assert!(translate(Stage::Pixel, &QSAD).is_err());
}

#[test]
fn masked_byte_sad_skips_zero_reference_bytes() {
    // v_msad_u8 v0, v1, v2, v3
    let hlsl = translate(Stage::Compute, &[0xd571_0000, 0x040e_0501]).unwrap();
    assert_eq!(hlsl.matches("((v2 >> 16u) & 0xffu) != 0u ?").count(), 1, "{hlsl}");
    assert!(!hlsl.contains("((v1 >> 16u) & 0xffu) != 0u"), "{hlsl}");
    // v_msad_u8 v0, v1, v2, 0: an inline accumulator is not accepted.
    assert!(translate(Stage::Compute, &[0xd571_0000, 0x0202_0501]).is_err());
    assert!(translate(Stage::Pixel, &[0xd571_0000, 0x040e_0501]).is_err());
}
