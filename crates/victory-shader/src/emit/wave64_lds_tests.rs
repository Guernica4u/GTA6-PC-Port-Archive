use super::*;
use crate::io;
use crate::ir;
use crate::native::{ComputeIo, HwStage, NativeProgram, StageIo};
use crate::resource;
use crate::structure;

const S_ENDPGM: u32 = 0xBF81_0000;

fn lower_wave_cs(words: &[u32], threads: [u32; 3], lds_dwords: u32) -> Lowered {
    let code: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let lds_granules = lds_dwords.div_ceil(128).max(1);
    let native = NativeProgram {
        stage: HwStage::Cs,
        wave64: true,
        vgprs: 256,
        layout: vec![],
        used: vec![],
        io: StageIo::Cs(ComputeIo {
            // lds_size field in COMPUTE_PGM_RSRC2 is bits [15:23] in 128-dword granules.
            rsrc2: lds_granules << 15,
            threads,
        }),
        code,
    };
    let p = ir::build(&native.code, true, 256).unwrap();
    let st = structure::structure(&p).unwrap();
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).unwrap();
    let io = io::raise(&p, &native).unwrap();
    lower(&Parts {
        native: &native,
        p: &p,
        st: &st,
        io: &io,
        res: &res,
        inputs: vec![],
        outputs: vec![],
        prims: vec![],
        mesh_stride: 0,
        view_marks: false,
    })
    .unwrap()
}

#[test]
fn wave64_lds_within_32_threads_drops_wave_size() {
    // Wave64 CS with <= 32 threads and uniform ds_write/ds_read fits in one Wave32 warp.
    let words = [
        0x7E00_0280, // v_mov_b32 v0, 0 (addr)
        0x7E02_0287, // v_mov_b32 v1, 7 (data)
        0xD834_0000, // ds_write_b32 v0, v1
        0x0000_0100, // v1 in src0
        0xD8D8_0000, // ds_read_b32 v2, v0
        0x0200_0000, // vdst = v2
        S_ENDPGM,
    ];
    let lowered = lower_wave_cs(&words, [32, 1, 1], 16);
    assert_eq!(lowered.wave_size, None);
    assert!(!lowered.lds_wave32_sync);
}

#[test]
fn wave64_lds_64_threads_uniform_drops_wave_size_and_emits_barriers() {
    // Wave64 CS with 64 threads and uniform ds_write/ds_read uses DXIL barriers instead of WaveSize(64).
    let words = [
        0x7E00_0280, // v_mov_b32 v0, 0 (addr)
        0x7E02_0287, // v_mov_b32 v1, 7 (data)
        0xD834_0000, // ds_write_b32 v0, v1
        0x0000_0100, // v1 in src0
        0xD8D8_0000, // ds_read_b32 v2, v0
        0x0200_0000, // vdst = v2
        S_ENDPGM,
    ];
    let lowered = lower_wave_cs(&words, [64, 1, 1], 16);
    assert_eq!(lowered.wave_size, None);
    assert!(lowered.lds_wave32_sync);
}

#[test]
fn wave64_lds_64_threads_guarded_keeps_wave_size_64() {
    // Divergent EXEC-guarded LDS access cannot safely insert a group barrier; keep WaveSize(64).
    let words = [
        0x7E02_0287, // v_mov_b32 v1, 7
        0x7D88_00A8, // v_cmp_gt_u32 vcc, 40, v0 (v0 = local thread id x)
        0xBE94_246A, // s_and_saveexec_b64 s[20:21], vcc
        0xD834_0000, // ds_write_b32 v0, v1
        0x0000_0100,
        0xBEFE_0414, // s_mov_b64 exec, s[20:21]
        S_ENDPGM,
    ];
    let lowered = lower_wave_cs(&words, [64, 1, 1], 16);
    assert_eq!(lowered.wave_size, Some(64));
    assert!(!lowered.lds_wave32_sync);
}
