//! Console wave64 compute programs translated for a host of 32-lane waves
//! (`lower::halves`): what reads within a 32-lane half runs exactly in two
//! host waves of 32, what reads across them is refused with its reason.
//!
//! The GPU test runs one program on every hardware adapter: in halves
//! everywhere (`WaveSize(32)`), and also as one wave64 (`WaveSize(64)`)
//! where the adapter has 64-lane waves (AMD), which is the console's own
//! form; the results must be the console's, computed here, in both.

use super::host::{Dispatch, Host, Root, Stride, View};
use super::{lower, validator};
use crate::native::{
    ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource,
};
use crate::{gfx10, io, ir, resource, structure};

const S_ENDPGM: u32 = 0xBF81_0000;
const S_WAITCNT_LGKM: u32 = 0xBF8C_C07F; // s_waitcnt lgkmcnt(0)

/// `s[40:43]`: the raw UAV at table dword 16 (s0 is the table's address).
const PROLOGUE: [u32; 5] = [
    0xBE84_0300, // s_mov_b32 s4, s0
    0xBE85_0380, // s_mov_b32 s5, 0
    0xF408_0A02,
    0xFA00_0040, // s_load_dwordx4 s[40:43], s[4:5], 0x40
    S_WAITCNT_LGKM,
];

/// `buffer_store_dword v<vdata>, v16, s[40:43], 0 offen offset:<offset>`.
const fn store(vdata: u8, offset: u32) -> [u32; 2] {
    [0xE070_1000 | offset, 0x800A_0010 | (vdata as u32) << 8]
}

/// Per thread `l` of a group of 64 (one console wave64), eight dwords at
/// `l * 32`: its thread id, `v_mbcnt` of all lanes (its lane), `l ^ 5`
/// through `ds_swizzle` (xor mode, within 32 lanes), `l ^ 9` through
/// `ds_bpermute_b32` (within the half), `0x777` written by `v_writelane` to
/// lane 40 only, whether the constant mask -4 (every lane but 0 and 1; lane
/// 32 is bit 0 of its high dword) has its lane, and `l ^ 1` through DPP
/// (`quad_perm:[1,0,3,2]`).
fn in_halves() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0xD765_0001,
        0x0001_00C1, // v_mbcnt_lo_u32_b32 v1, -1, 0
        0xD766_0001,
        0x0002_02C1, // v_mbcnt_hi_u32_b32 v1, -1, v1
        0xD8D4_141F,
        0x0200_0000, // ds_swizzle_b32 v2, v0 offset:swizzle(BITMASK_PERM xor 5)
        0x3A08_0089, // v_xor_b32 v4, 9, v0
        0x3408_0882, // v_lshlrev_b32 v4, 2, v4
        0xDACC_0000,
        0x0300_0004, // ds_bpermute_b32 v3, v4, v0
        0xBE88_03FF,
        0x0000_0777, // s_mov_b32 s8, 0x777
        0x7E0A_0280, // v_mov_b32 v5, 0
        0xD761_0005,
        0x0001_5008, // v_writelane_b32 v5, s8, 40
        0xBE94_04C4, // s_mov_b64 s[20:21], -4
        0xD501_0006,
        0x0051_0280, // v_cndmask_b32 v6, 0, 1, s[20:21]
        0x7E0E_02FA,
        0xFF00_B100, // v_mov_b32 v7, v0 quad_perm:[1,0,3,2] (DPP)
        S_WAITCNT_LGKM,
    ]);
    for (vdata, slot) in [(0, 0), (1, 1), (2, 2), (3, 3), (5, 4), (6, 5), (7, 6)] {
        w.extend(store(vdata, 4 * slot));
    }
    w.push(S_ENDPGM);
    w
}

/// What the console gives [`in_halves`]'s lane `l`.
fn console(l: u32) -> [u32; 8] {
    [
        l,
        l,
        l ^ 5,
        l ^ 9,
        if l == 40 { 0x777 } else { 0 },
        u32::from(l >= 2),
        l ^ 1,
        0,
    ]
}

fn native(words: &[u32], wave64: bool, threads: u32, lds_bytes: u32) -> NativeProgram {
    let range = |kind, table_offset_dwords, dwords| UsedResource {
        kind,
        root: RootType::Table,
        table_offset_dwords,
        lower_bound: 0,
        stride: 0,
        dwords,
        user_data: 0,
        slot: 0,
    };
    NativeProgram {
        stage: HwStage::Cs,
        wave64,
        vgprs: 32,
        layout: vec![RootParam {
            root: RootType::Table,
            table_offset_dwords: 0,
            sgprs: 1,
            base_register: 0,
            user_data: 0,
            slot: 0,
        }],
        used: vec![
            range(ResourceType::Srv, 0, 8),
            range(ResourceType::Srv, 8, 4),
            range(ResourceType::Uav, 16, 4),
        ],
        io: StageIo::Cs(ComputeIo {
            // One user-data SGPR; LDS in blocks of 512 bytes (bits 15-23).
            rsrc2: 1 << 1 | lds_bytes.div_ceil(512) << 15,
            threads: [threads, 1, 1],
        }),
        code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    }
}

fn lowered(
    native: &NativeProgram,
    host_wave32: bool,
) -> Result<lower::Lowered, crate::ShaderError> {
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let st = structure::structure(&p).expect("structure");
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("track");
    let io = io::raise(&p, native).expect("io::raise");
    lower::lower(&lower::Parts {
        native,
        p: &p,
        st: &st,
        io: &io,
        res: &res,
        inputs: Vec::new(),
        outputs: Vec::new(),
        prims: Vec::new(),
        mesh_stride: 0,
        view_marks: false,
        host_wave32,
    })
}

fn shader(l: lower::Lowered) -> super::HostShader {
    super::HostShader {
        stage: HwStage::Cs,
        dxil: l.dxil,
        user_data: l.user_data,
        static_samplers: Vec::new(),
        samplers: l.samplers,
        root_cbvs: l.root_cbvs,
        root_srvs: l.root_srvs,
        root_uavs: l.root_uavs,
        memory: l.memory,
        memory_writes: l.memory_writes,
        view_addresses: l.view_addresses,
        heap: l.heap,
        heap_writes: l.heap_writes,
        view_marks: l.view_marks,
    }
}

fn refusal(words: &[u32], threads: u32, lds_bytes: u32) -> String {
    match lowered(&native(words, true, threads, lds_bytes), true) {
        Ok(_) => "translated".into(),
        Err(e) => e.to_string(),
    }
}

/// The hand-encoded words are the instructions their comments name.
#[test]
fn the_test_programs_are_what_their_comments_say() {
    let programs: [(Vec<u32>, &[&str]); 10] = [
        (
            lane_reads(),
            &[
                "v_readlane_b32 s12, v0, 40",
                "s_mov_b32 s20, 63",
                "v_readlane_b32 s13, v0, s20",
                "v_readlane_b32 s14, v0, 5",
                "buffer_store_dword v4, v16, s[40:43], 0 offen offset:8",
            ],
        ),
        (
            lane_read_in_a_branch(),
            &[
                "v_cmp_gt_u32 vcc, 20, v0",
                "s_and_saveexec_b64 s[20:21], vcc",
                "s_cbranch_execz",
                "s_mov_b64 exec, s[20:21]",
            ],
        ),
        (
            lane_read_under_a_uniform_branch(),
            &["s_cmp_eq_u32 s5, 0", "s_cbranch_scc0"],
        ),
        (
            small_group(),
            &[
                "v_cmp_gt_u32 vcc, 10, v0",
                "s_bcnt1_i32_b64 s12, vcc",
                "v_mov_b32 v7, vcc_hi",
                "v_readlane_b32 s13, v0, 5",
                "s_or_saveexec_b64 s[0:1], -1",
                "v_add_nc_u32 v1, v1, v1 row_shr:8",
                "v_readlane_b32 s2, v1, 47",
                "s_mov_b64 exec, s[0:1]",
                "buffer_store_dword v8, v16, s[40:43], 0 offen offset:12",
            ],
        ),
        (
            every_id(3, 2),
            &[
                "v_lshlrev_b32 v3, 2, v2",
                "v_add_nc_u32 v3, v1, v3",
                "v_lshlrev_b32 v3, 3, v3",
                "v_add_nc_u32 v2, v0, v3",
                "v_lshlrev_b32 v16, 2, v2",
                "v_mov_b32 v3, 1",
            ],
        ),
        (
            in_halves(),
            &[
                "v_mbcnt_lo_u32_b32 v1, -1, 0",
                "v_mbcnt_hi_u32_b32 v1, -1, v1",
                "ds_swizzle_b32 v2, v0",
                "v_xor_b32 v4, 9, v0",
                "ds_bpermute_b32 v3, v4, v0",
                "v_writelane_b32 v5, s8, 40",
                "v_cndmask_b32 v6, 0, 1, s[20:21]",
                "v_mov_b32 v7, v0 quad_perm:[1,0,3,2]",
                "buffer_store_dword v7, v16, s[40:43], 0 offen offset:24",
            ],
        ),
        (
            dpp_row_mask(),
            &[
                "v_mov_b32 v7, -1",
                "v_mov_b32 v7, v0 quad_perm:[1,0,3,2] row_mask:0xc",
                "buffer_store_dword v7, v16, s[40:43], 0 offen",
            ],
        ),
        (
            first_lane_of_the_thread_id(),
            &[
                "v_mbcnt_lo_u32_b32 v1, -1, 0",
                "v_readfirstlane_b32 s12, v0",
                "v_mov_b32 v2, s12",
            ],
        ),
        (
            first_lane_of_a_buffer_load(3),
            &[
                "s_load_dwordx4 s[44:47], s[4:5], 0x000020",
                "v_mov_b32 v3, 4",
                "buffer_load_dword v4, v3, s[44:47], 0 offen",
                "s_waitcnt vmcnt(0)",
                "v_readfirstlane_b32 s12, v4",
                "v_mov_b32 v5, s12",
            ],
        ),
        (
            first_lane_alike(),
            &[
                "v_lshrrev_b32 v2, 6, v0",
                "v_readfirstlane_b32 s12, v2",
                "v_mov_b32 v3, s12",
                "buffer_store_dword v1, v16, s[40:43], 0 offen offset:4",
            ],
        ),
    ];
    for (words, wants) in programs {
        let code: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let all = gfx10::decode(&code)
            .expect("decodes")
            .iter()
            .map(|i| i.text(true))
            .collect::<Vec<_>>()
            .join("\n");
        for want in wants {
            assert!(all.contains(want), "{want:?} not in:\n{all}");
        }
    }
}

/// In halves the program declares waves of 32; without, the console's 64.
#[test]
fn a_wave64_program_reading_lanes_declares_32_lane_waves_in_halves() {
    let n = native(&in_halves(), true, 64, 0);
    let halves = lowered(&n, true).expect("halves");
    let whole = lowered(&n, false).expect("one wave64");
    assert_eq!(halves.wave_size, Some(32));
    assert_eq!(whole.wave_size, Some(64));
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        v.validate(&mut halves.dxil.clone())
            .expect("DXIL validation failed");
    }
    // A wave32 program is the console's either way.
    let n32 = native(&in_halves(), false, 32, 0);
    assert_eq!(lowered(&n32, true).expect("wave32").wave_size, Some(32));
}

/// Reads across the halves are refused where they are made, with a reason
/// naming the host's waves; the same programs translate for 64-lane hosts.
#[test]
fn reads_across_32_lane_halves_are_refused_with_their_reason() {
    let readlane = [
        PROLOGUE.as_slice(),
        &[
            0x3420_0085, // v_lshlrev_b32 v16, 5, v0
            0xD760_000C,
            0x0001_5100, // v_readlane_b32 s12, v0, 40
            0x7E02_020C, // v_mov_b32 v1, s12
        ],
        &store(1, 0),
        &[S_ENDPGM],
    ]
    .concat();
    // A lane read where every thread of the group meets is traded between
    // the halves; the same program translates for 64-lane hosts.
    assert_eq!(refusal(&readlane, 64, 0), "translated");
    assert!(lowered(&native(&readlane, true, 64, 0), false).is_ok());
    // Inside a branch only lanes below 20 take, the halves cannot meet.
    let why = refusal(&lane_read_in_a_branch(), 64, 0);
    assert!(
        why.contains("where not every thread of the group meets"),
        "{why}"
    );
    // Under a branch on a uniform condition both halves of a console wave
    // go alike, but two console waves of one group need not.
    let uniform = lane_read_under_a_uniform_branch();
    assert_eq!(refusal(&uniform, 64, 0), "translated");
    let why = refusal(&uniform, 128, 0);
    assert!(
        why.contains("where not every thread of the group meets"),
        "{why}"
    );

    let first = [
        PROLOGUE.as_slice(),
        &[
            0x3420_0085, // v_lshlrev_b32 v16, 5, v0
            0x7E18_0500, // v_readfirstlane_b32 s12, v0
            0x7E02_020C, // v_mov_b32 v1, s12
        ],
        &store(1, 0),
        &[S_ENDPGM],
    ]
    .concat();
    // A first-lane read alone needs no console lanes: the host wave's
    // first lane, in halves or not.
    assert_eq!(refusal(&first, 64, 0), "translated");

    // With console lanes the program runs in halves, where lane 32 is the
    // upper host wave's first lane: a value that differs between the halves
    // (the thread id) would read 32 there, the console 0. In a group of 32
    // there is no upper half.
    let why = refusal(&first_lane_of_the_thread_id(), 64, 0);
    assert!(
        why.contains("a first-lane read of a value that may differ between 32-lane halves"),
        "{why}"
    );
    assert_eq!(refusal(&first_lane_of_the_thread_id(), 32, 0), "translated");
    // A first-lane read nothing uses costs nothing.
    let unused_first = [
        PROLOGUE.as_slice(),
        &[
            0x3420_0085, // v_lshlrev_b32 v16, 5, v0
            0xD765_0001,
            0x0001_00C1, // v_mbcnt_lo_u32_b32 v1, -1, 0
            0x7E18_0500, // v_readfirstlane_b32 s12, v0
        ],
        &store(1, 0),
        &[S_ENDPGM],
    ]
    .concat();
    assert_eq!(refusal(&unused_first, 64, 0), "translated");
    // A buffer dword loaded at an offset the same in every lane, through a
    // scalar descriptor, is one value: its first lane is the console's.
    assert_eq!(
        refusal(&first_lane_of_a_buffer_load(3), 64, 0),
        "translated"
    );
    let why = refusal(&first_lane_of_a_buffer_load(0), 64, 0);
    assert!(
        why.contains("a first-lane read of a value that may differ between 32-lane halves"),
        "{why}"
    );

    // A value read across halves that nothing uses costs nothing.
    let unused = [
        PROLOGUE.as_slice(),
        &[
            0x3420_0085, // v_lshlrev_b32 v16, 5, v0
            0xD760_000C,
            0x0001_5100, // v_readlane_b32 s12, v0, 40
            0xD765_0001,
            0x0001_00C1, // v_mbcnt_lo_u32_b32 v1, -1, 0
        ],
        &store(1, 0),
        &[S_ENDPGM],
    ]
    .concat();
    assert_eq!(refusal(&unused, 64, 0), "translated");
}

/// LDS shared by a group that is one console wave64 (no barriers between
/// its lanes) cannot be split into two host waves; one of 32 threads or
/// fewer is one host wave, and a group of several console waves keeps the
/// console's barriers.
#[test]
fn lds_of_a_one_wave64_group_is_refused_in_halves() {
    let lds = [
        PROLOGUE.as_slice(),
        &[
            0x3420_0085, // v_lshlrev_b32 v16, 5, v0
            0x3404_0082, // v_lshlrev_b32 v2, 2, v0
            0xD765_0001,
            0x0001_00C1, // v_mbcnt_lo_u32_b32 v1, -1, 0
            0xD834_0000,
            0x0000_0102, // ds_write_b32 v2, v1
            S_WAITCNT_LGKM,
            0xD8D8_0000,
            0x0300_0002, // ds_read_b32 v3, v2
            S_WAITCNT_LGKM,
        ],
        &store(3, 0),
        &[S_ENDPGM],
    ]
    .concat();
    let why = refusal(&lds, 64, 256);
    assert!(
        why.contains("LDS one console wave64 shares without barriers"),
        "{why}"
    );
    assert_eq!(refusal(&lds, 32, 256), "translated");
    assert_eq!(refusal(&lds, 128, 512), "translated");
}

/// Runs `words`, one group of `threads` of a wave64 program whose lanes store
/// `per_lane` dwords at `l * 32`, on every hardware adapter: in halves, and
/// as one wave64 where the adapter runs 64-lane waves. A line per lane whose
/// dwords differ from `want` (the console's, `per_lane` per lane).
fn differences(words: &[u32], threads: u32, per_lane: usize, want: &[u32]) -> Vec<String> {
    let n = native(words, true, threads, 0);
    let forms = [
        ("halves", shader(lowered(&n, true).expect("halves"))),
        ("wave64", shader(lowered(&n, false).expect("one wave64"))),
    ];
    let views = [
        View::Texture(1, 1, &[[0.0; 4]]),
        View::Buffer {
            data: &[0; 4],
            stride: Stride::Structured(16),
        },
        View::RawUav(threads * 8),
    ];
    let _one = super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let hosts = Host::each_hardware();
    if hosts.is_empty() {
        eprintln!("no SM 6.6 hardware device; skipped");
    }
    let mut failed = Vec::new();
    for (adapter, host) in &hosts {
        let wave32 = host.wave32().unwrap_or(false);
        for (form, shader) in &forms {
            let label = format!("{adapter} ({form})");
            let Ok((root, pso)) = host.compute_pipeline(shader) else {
                // Only the console's own form may be refused, and only
                // where the host's waves are 32 lanes.
                assert!(*form == "wave64" && wave32, "{label}: no pipeline");
                eprintln!("{label}: no pipeline for 64-lane waves; skipped");
                continue;
            };
            let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
            let out = host
                .dispatch(
                    &root,
                    &pso,
                    &Dispatch {
                        roots: vec![(0, Root::Constants(ud))],
                        views: &views,
                        samplers: &[],
                        groups: [1, 1, 1],
                    },
                )
                .unwrap_or_else(|e| panic!("{label}: dispatch failed: {e}"));
            let wrong: Vec<String> = out[0]
                .chunks(8)
                .zip(want.chunks(per_lane))
                .enumerate()
                .filter(|(_, (got, want))| got[..per_lane] != **want)
                .map(|(l, (got, want))| {
                    format!(
                        "{label}: lane {l}: {:?}, the console {want:?}",
                        &got[..per_lane]
                    )
                })
                .collect();
            eprintln!(
                "{label}: {} lanes of {threads} differ from the console",
                wrong.len()
            );
            failed.extend(wrong);
        }
    }
    failed
}

/// A DPP row mask names the console's rows 0-3: with `row_mask:0xc` only
/// rows 2 and 3 (lanes 32-63) take `l ^ 1`, rows 0 and 1 keep -1.
fn dpp_row_mask() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0x7E0E_02C1, // v_mov_b32 v7, -1
        0x7E0E_02FA,
        0xCF00_B100, // v_mov_b32 v7, v0 quad_perm:[1,0,3,2] row_mask:0xc (DPP)
    ]);
    w.extend(store(7, 0));
    w.push(S_ENDPGM);
    w
}

#[test]
fn dpp_row_masks_name_the_console_rows_on_every_gpu() {
    let want: Vec<u32> = (0..64)
        .map(|l| if l >= 32 { l ^ 1 } else { u32::MAX })
        .collect();
    let failed = differences(&dpp_row_mask(), 64, 1, &want);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// `v_readfirstlane_b32` of the thread id, with console lanes (`v_mbcnt`):
/// the console's first lane is lane 0 of the wave.
fn first_lane_of_the_thread_id() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0xD765_0001,
        0x0001_00C1, // v_mbcnt_lo_u32_b32 v1, -1, 0
        0x7E18_0500, // v_readfirstlane_b32 s12, v0
        0x7E04_020C, // v_mov_b32 v2, s12
    ]);
    w.extend(store(2, 0));
    w.push(S_ENDPGM);
    w
}

/// `v_readfirstlane_b32` of a dword loaded from the buffer at table dword 8,
/// at the byte offset in `v<offset>`: `v3` holds 4 in every lane, `v0` the
/// thread id.
fn first_lane_of_a_buffer_load(offset: u32) -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0xF408_0B02,
        0xFA00_0020, // s_load_dwordx4 s[44:47], s[4:5], 0x20
        S_WAITCNT_LGKM,
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0xD765_0001,
        0x0001_00C1, // v_mbcnt_lo_u32_b32 v1, -1, 0
        0x7E06_0284, // v_mov_b32 v3, 4
        0xE030_1000,
        0x800B_0400 | offset, // buffer_load_dword v4, v<offset>, s[44:47], 0 offen
        0xBF8C_3F70,          // s_waitcnt vmcnt(0)
        0x7E18_0504,          // v_readfirstlane_b32 s12, v4
        0x7E0A_020C,          // v_mov_b32 v5, s12
    ]);
    w.extend(store(5, 0));
    w.push(S_ENDPGM);
    w
}

/// Per thread `l`, three dwords at `l * 32`: the thread ids of lanes 40, 63
/// (named by an SGPR) and 5 of its console wave, through `v_readlane_b32`.
fn lane_reads() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0xD760_000C,
        0x0001_5100, // v_readlane_b32 s12, v0, 40
        0x7E04_020C, // v_mov_b32 v2, s12
        0xBE94_03BF, // s_mov_b32 s20, 63
        0xD760_000D,
        0x0000_2900, // v_readlane_b32 s13, v0, s20
        0x7E06_020D, // v_mov_b32 v3, s13
        0xD760_000E,
        0x0001_0B00, // v_readlane_b32 s14, v0, 5
        0x7E08_020E, // v_mov_b32 v4, s14
    ]);
    for (vdata, slot) in [(2, 0), (3, 1), (4, 2)] {
        w.extend(store(vdata, 4 * slot));
    }
    w.push(S_ENDPGM);
    w
}

/// Lane 40's thread id, read inside a branch only threads below 20 take.
fn lane_read_in_a_branch() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0x7D88_0094, // v_cmp_gt_u32 vcc, 20, v0
        0xBE94_246A, // s_and_saveexec_b64 s[20:21], vcc
        0xBF88_0003, // s_cbranch_execz 3
        0xD760_000C,
        0x0001_5100, // v_readlane_b32 s12, v0, 40
        0x7E04_020C, // v_mov_b32 v2, s12
        0xBEFE_0414, // s_mov_b64 exec, s[20:21]
    ]);
    w.extend(store(2, 0));
    w.push(S_ENDPGM);
    w
}

/// Lane 40's thread id, read under a branch on a scalar compare.
fn lane_read_under_a_uniform_branch() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0xBF06_8005, // s_cmp_eq_u32 s5, 0
        0xBF84_0003, // s_cbranch_scc0 3
        0xD760_000C,
        0x0001_5100, // v_readlane_b32 s12, v0, 40
        0x7E04_020C, // v_mov_b32 v2, s12
    ]);
    w.extend(store(2, 0));
    w.push(S_ENDPGM);
    w
}

/// [`lane_reads`] on every hardware adapter, in halves (traded between the
/// two host waves) and as one wave64 where the adapter runs those: in groups
/// of one and of two console waves, every lane gets its console wave's lanes.
#[test]
fn lane_reads_across_halves_match_the_console_on_every_gpu() {
    let mut failed = Vec::new();
    for threads in [64, 128] {
        let want: Vec<u32> = (0..threads)
            .flat_map(|l| {
                let base = l & !63;
                [base + 40, base + 63, base + 5]
            })
            .collect();
        failed.extend(differences(&lane_reads(), threads, 3, &want));
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Per thread `l` of a group of 128 (two console wave64s), two dwords at
/// `l * 32`: the first lane of `l >> 6` (one value per console wave, the
/// same in both its halves) and the lane `v_mbcnt` counts.
fn first_lane_alike() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085, // v_lshlrev_b32 v16, 5, v0
        0xD765_0001,
        0x0001_00C1, // v_mbcnt_lo_u32_b32 v1, -1, 0
        0xD766_0001,
        0x0002_02C1, // v_mbcnt_hi_u32_b32 v1, -1, v1
        0x2C04_0086, // v_lshrrev_b32 v2, 6, v0
        0x7E18_0502, // v_readfirstlane_b32 s12, v2
        0x7E06_020C, // v_mov_b32 v3, s12
    ]);
    w.extend(store(3, 0));
    w.extend(store(1, 4));
    w.push(S_ENDPGM);
    w
}

/// First-lane reads kept in halves give the console's value on every
/// hardware adapter: the thread id's in a group of 32 (one host wave holds
/// the whole console wave), and one value per console wave in a group of
/// 128, where each half reads its console wave's.
#[test]
fn first_lanes_kept_in_halves_match_the_console_on_every_gpu() {
    let mut failed = differences(&first_lane_of_the_thread_id(), 32, 1, &[0; 32]);
    let want: Vec<u32> = (0..128).flat_map(|l| [l >> 6, l & 63]).collect();
    failed.extend(differences(&first_lane_alike(), 128, 2, &want));
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A group of `[x, y, z]` threads (thread ids in v0-v2) running `words`.
fn native_shaped(words: &[u32], [x, y, z]: [u32; 3]) -> NativeProgram {
    let mut n = native(words, true, x * y * z, 0);
    if let StageIo::Cs(cs) = &mut n.io {
        cs.threads = [x, y, z];
        cs.rsrc2 |= 2 << 11; // TIDIG_COMP_CNT 2: v1 holds y, v2 holds z
    }
    n
}

/// Each thread of a group `1 << sx` wide and `1 << sy` high (any depth)
/// stores 1 at its flat id `(z << sy | y) << sx | x`, with console lanes
/// (`v_mbcnt`): a thread id two threads share leaves another one unstored.
fn every_id(sx: u32, sy: u32) -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0xD765_0005,
        0x0001_00C1,              // v_mbcnt_lo_u32_b32 v5, -1, 0
        0x3406_0400 | (128 + sy), // v_lshlrev_b32 v3, sy, v2
        0x4A06_0701,              // v_add_nc_u32 v3, v1, v3
        0x3406_0600 | (128 + sx), // v_lshlrev_b32 v3, sx, v3
        0x4A04_0700,              // v_add_nc_u32 v2, v0, v3
        0x3420_0482,              // v_lshlrev_b32 v16, 2, v2
        0x7E06_0281,              // v_mov_b32 v3, 1
    ]);
    w.extend(store(3, 0));
    w.push(S_ENDPGM);
    w
}

/// A group of several rows whose rows a 32-lane host wave may tile is
/// refused in halves; narrower rows, and one row, are not.
#[test]
fn groups_a_host_wave_may_tile_are_refused_in_halves() {
    let why =
        |shape: [u32; 3], sx, sy| match lowered(&native_shaped(&every_id(sx, sy), shape), true) {
            Ok(_) => "translated".to_string(),
            Err(e) => e.to_string(),
        };
    for (shape, sx, sy) in [
        ([32, 2, 1], 5, 1),
        ([64, 4, 1], 6, 2),
        ([32, 2, 2], 5, 1),
        ([6, 6, 1], 3, 3),
    ] {
        let w = why(shape, sx, sy);
        assert!(
            w.contains("a group whose rows a 32-lane host wave may tile"),
            "{shape:?}: {w}"
        );
    }
    for (shape, sx, sy) in [
        ([8, 8, 1], 3, 3),
        ([16, 2, 1], 4, 1),
        ([4, 4, 4], 2, 2),
        ([64, 1, 1], 6, 0),
    ] {
        assert_eq!(why(shape, sx, sy), "translated", "{shape:?}");
    }
}

/// The groups halves keep give every thread its own id on every hardware
/// adapter, in halves and as one wave64 where the adapter runs those.
#[test]
fn every_thread_keeps_its_own_id_in_halves_on_every_gpu() {
    let _one = super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let hosts = Host::each_hardware();
    if hosts.is_empty() {
        eprintln!("no SM 6.6 hardware device; skipped");
    }
    let mut failed = Vec::new();
    for (shape, sx, sy) in [
        ([8, 8, 1], 3, 3),
        ([16, 2, 1], 4, 1),
        ([4, 4, 4], 2, 2),
        ([64, 1, 1], 6, 0),
    ] {
        let n = native_shaped(&every_id(sx, sy), shape);
        let threads = shape.iter().product::<u32>();
        for (form, halves) in [("halves", true), ("wave64", false)] {
            let s = shader(lowered(&n, halves).expect("lowers"));
            for (adapter, host) in &hosts {
                let label = format!("{adapter} {shape:?} ({form})");
                let Ok((root, pso)) = host.compute_pipeline(&s) else {
                    assert!(
                        !halves && host.wave32().unwrap_or(false),
                        "{label}: no pipeline"
                    );
                    continue;
                };
                let ud = vec![0u32; s.user_data_dwords().max(1) as usize];
                let views = [
                    View::Texture(1, 1, &[[0.0; 4]]),
                    View::Buffer {
                        data: &[0; 4],
                        stride: Stride::Structured(16),
                    },
                    View::RawUav(threads),
                ];
                let out = host
                    .dispatch(
                        &root,
                        &pso,
                        &Dispatch {
                            roots: vec![(0, Root::Constants(ud))],
                            views: &views,
                            samplers: &[],
                            groups: [1, 1, 1],
                        },
                    )
                    .unwrap_or_else(|e| panic!("{label}: dispatch failed: {e}"));
                let stored = out[0]
                    .iter()
                    .take(threads as usize)
                    .filter(|&&v| v == 1)
                    .count();
                eprintln!("{label}: {stored} of {threads} ids stored");
                if stored != threads as usize {
                    failed.push(format!("{label}: {stored} of {threads} ids stored"));
                }
            }
        }
    }
    assert!(
        failed.is_empty(),
        "{}",
        failed.join(
            "
"
        )
    );
}

const fn sop1(sdst: u8, op: u8, ssrc0: u8) -> u32 {
    0xBE80_0000 | (sdst as u32) << 16 | (op as u32) << 8 | ssrc0 as u32
}

const fn vop1(vdst: u8, op: u8, src0: u16) -> u32 {
    (0x3F << 25) | (vdst as u32) << 17 | (op as u32) << 9 | (src0 as u32 & 0x1FF)
}

const fn vop2(vdst: u8, op: u8, src0: u16, vsrc1: u8) -> u32 {
    (op as u32) << 25 | (vdst as u32) << 17 | (vsrc1 as u32) << 9 | (src0 as u32 & 0x1FF)
}

const fn dpp16(vsrc0: u8, ctrl: u16) -> u32 {
    0xFF << 24 | (ctrl as u32) << 8 | vsrc0 as u32
}

const fn vop3(vdst: u8, op: u16, src0: u16, src1: u16, src2: u16) -> [u32; 2] {
    [
        0xD400_0000 | (op as u32) << 16 | vdst as u32,
        (src0 as u32 & 0x1FF) | ((src1 as u32 & 0x1FF) << 9) | ((src2 as u32 & 0x1FF) << 18),
    ]
}

/// The exclusive prefix sum of `v0` over the active lanes, in `v2`, as the
/// console compiler expands `WavePrefixSum` in a wave64 (the section of
/// `wave::tests::exec_minus_two_masks_lane_zero_and_enforces_dpp_guard`):
/// row sums by DPP, the rows below added through lane reads, then shifted
/// by one lane. It writes `s[0:2]` and `v1`-`v5`.
fn prefix_sum_section() -> Vec<u32> {
    use crate::ir::EXEC;
    let mut w = vec![sop1(0, 0x25, 193)]; // s_or_saveexec_b64 s[0:1], -1
    w.extend(vop3(1, 0x101, 128, 256, 0)); // v_cndmask_b32 v1, 0, v0, s[0:1]
    for shift in [1u16, 2, 4, 8] {
        w.push(vop2(1, 0x25, 0xFA, 1)); // v_add_nc_u32 v1, v1, v1 row_shr:shift
        w.push(dpp16(1, 0x110 + shift));
    }
    // Each row adds the sums of the rows below it (lanes 15, 31, 47).
    for (lane, lo, hi) in [
        (15u16, 0xFFFF_0000u32, 0u32),
        (31, 0, 0x0000_FFFF),
        (47, 0, 0xFFFF_0000),
    ] {
        w.extend(vop3(2, 0x360, 257, 128 + lane, 0)); // v_readlane_b32 s2, v1, lane
        w.extend([sop1(EXEC, 0x03, 255), lo, sop1(EXEC + 1, 0x03, 255), hi]);
        w.push(vop2(1, 0x25, 2, 1)); // v_add_nc_u32 v1, s2, v1
    }
    w.push(sop1(EXEC, 0x04, 193)); // s_mov_b64 exec, -1
    w.push(vop1(3, 0x01, 257)); // v_mov_b32 v3, v1
    w.push(vop1(3, 0x01, 0xFA)); // v_mov_b32 v3, v1 row_shr:1
    w.push(dpp16(1, 0x111));
    // The first lane of each row takes the last lane of the row below.
    for (lane, lo, hi) in [
        (15u16, 0x0001_0000u32, 0u32),
        (31, 0, 1),
        (47, 0, 0x0001_0000),
    ] {
        w.extend(vop3(2, 0x360, 257, 128 + lane, 0)); // v_readlane_b32 s2, v1, lane
        w.extend([sop1(EXEC, 0x03, 255), lo, sop1(EXEC + 1, 0x03, 255), hi]);
        w.push(vop1(3, 0x01, 2)); // v_mov_b32 v3, s2
    }
    w.push(sop1(EXEC, 0x04, 193)); // s_mov_b64 exec, -1
    w.push(vop1(2, 0x01, 128)); // v_mov_b32 v2, 0
    w.push(vop1(5, 0x01, 135)); // v_mov_b32 v5, 7
    w.push(sop1(EXEC, 0x04, 194)); // s_mov_b64 exec, -2
    w.push(vop1(2, 0x01, 259)); // v_mov_b32 v2, v3
    w.push(sop1(EXEC, 0x04, 0)); // s_mov_b64 exec, s[0:1]
    w
}

/// Per thread `l`, four dwords at `l * 32`: the exclusive prefix sum of the
/// thread ids below it, the number of threads below 10 (`s_bcnt1_i32_b64`
/// of a compare's mask), that mask's high dword and lane 5's thread id.
fn small_group() -> Vec<u32> {
    let mut w = PROLOGUE.to_vec();
    w.extend([
        0x3420_0085,         // v_lshlrev_b32 v16, 5, v0
        0x7D88_008A,         // v_cmp_gt_u32 vcc, 10, v0
        sop1(12, 0x10, 106), // s_bcnt1_i32_b64 s12, vcc
        vop1(6, 0x01, 12),   // v_mov_b32 v6, s12
        vop1(7, 0x01, 107),  // v_mov_b32 v7, vcc_hi
    ]);
    w.extend(vop3(13, 0x360, 256, 128 + 5, 0)); // v_readlane_b32 s13, v0, 5
    w.push(vop1(8, 0x01, 13)); // v_mov_b32 v8, s13
    w.extend(prefix_sum_section());
    for (vdata, slot) in [(2, 0), (6, 1), (7, 2), (8, 3)] {
        w.extend(store(vdata, 4 * slot));
    }
    w.push(S_ENDPGM);
    w
}

/// In a group of 32 threads or fewer one host wave holds the whole console
/// wave: masks read as data, wave prefix sums and a lane read of a constant
/// lane below 32 translate in halves; the same reads in a group of 64, and a
/// lane read past the group, are refused.
#[test]
fn a_group_one_host_wave_holds_reads_nothing_across_halves() {
    assert_eq!(refusal(&small_group(), 32, 0), "translated");
    assert_eq!(refusal(&small_group(), 16, 0), "translated");
    let why = refusal(&small_group(), 64, 0);
    assert!(why.contains("(wave64 on a 32-lane host)"), "{why}");
    let readlane = [
        PROLOGUE.as_slice(),
        &[
            0x3420_0085, // v_lshlrev_b32 v16, 5, v0
            0xD760_000C,
            0x0001_5100, // v_readlane_b32 s12, v0, 40
            0x7E02_020C, // v_mov_b32 v1, s12
        ],
        &store(1, 0),
        &[S_ENDPGM],
    ]
    .concat();
    let why = refusal(&readlane, 32, 0);
    assert!(
        why.contains("a lane read that may cross 32-lane halves"),
        "{why}"
    );
}

/// [`small_group`] in a group of 32 on every hardware adapter, in halves and
/// as one wave64 where the adapter runs those: the console's results.
#[test]
fn a_group_one_host_wave_holds_matches_the_console_on_every_gpu() {
    let want: Vec<u32> = (0..32)
        .flat_map(|l| [l * (l.max(1) - 1) / 2, 10, 0, 5])
        .collect();
    let failed = differences(&small_group(), 32, 4, &want);
    assert!(
        failed.is_empty(),
        "{}",
        failed.join(
            "
"
        )
    );
}

/// [`in_halves`] on every hardware adapter: in halves, and as one wave64
/// where the adapter runs 64-lane waves; both give the console's results.
#[test]
fn wave64_programs_in_halves_match_the_console_on_every_gpu() {
    let want: Vec<u32> = (0..64).flat_map(console).collect();
    let failed = differences(&in_halves(), 64, 8, &want);
    assert!(
        failed.is_empty(),
        "{}",
        failed.join(
            "
"
        )
    );
}
