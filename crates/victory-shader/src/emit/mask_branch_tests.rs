//! A branch on the SCC of a mask operation nothing else reads (`s_and
//! sX, vcc, exec` then `s_cbranch_scc0/1`) is the console wave's decision:
//! "some lane of the mask is set", the same arm for every active lane,
//! whatever the width of the host's waves. Programs by [`super::mask_branch_repro`].

use super::host::{Dispatch, Host, Root, View};
use super::mask_branch_repro::{Program, program};
use crate::native::RootType;

const KINDS: [Program; 4] = [
    Program::Branch,
    Program::BranchScc1,
    Program::StoreOnScc0,
    Program::StoreOnScc1,
];

fn emit(kind: Program, lanes: u32) -> super::HostShader {
    let n = program(kind, lanes);
    let (p, st) = crate::range::build(&n.patched_code(), n.wave64, n.vgprs).unwrap();
    let st = st.unwrap();
    let res = crate::resource::track(&p, &n.layout, n.stage, Some(&st)).unwrap();
    let io = crate::io::raise(&p, &n).unwrap();
    let bytes = crate::dxbc::build(&[]);
    let c = crate::Container::parse(&bytes).unwrap();
    super::emit(&c, &n, &p, &st, &io, &res).unwrap()
}

/// The branch's SCC is found, and only for a mask nothing else reads.
#[test]
fn only_a_branch_reads_the_scc_of_a_mask_it_is_found() {
    for lanes in [32, 64] {
        for kind in KINDS.into_iter().chain([Program::BranchMaskRead]) {
            let n = program(kind, lanes);
            let (p, st) = crate::range::build(&n.patched_code(), n.wave64, n.vgprs).unwrap();
            let st = st.unwrap();
            let want = usize::from(!matches!(kind, Program::BranchMaskRead));
            assert_eq!(st.any_sccs.len(), want, "{kind:?}, {lanes} lanes");
            drop(p);
        }
    }
}

/// The shaders with the wave's vote validate.
#[test]
fn the_branches_on_the_waves_vote_validate() {
    for lanes in [32, 64] {
        for kind in KINDS {
            let shader = emit(kind, lanes);
            if let Ok(mut v) = super::validator::Validator::load(
                super::validator::Validator::default_path().as_deref(),
            ) {
                v.validate(&mut shader.dxil.clone())
                    .unwrap_or_else(|e| panic!("{kind:?}, {lanes} lanes: {e}"));
            }
        }
    }
}

/// Whether the host has waves of exactly `lanes` lanes to run on.
fn runs_waves_of(host: &Host, lanes: u32) -> bool {
    use windows::Win32::Graphics::Direct3D12::*;
    let mut opts1 = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
    // SAFETY: a capability query on a live device, into a structure of the
    // size given.
    let queried = unsafe {
        host.device.CheckFeatureSupport(
            D3D12_FEATURE_D3D12_OPTIONS1,
            (&raw mut opts1).cast(),
            size_of_val(&opts1) as u32,
        )
    };
    queried.is_ok()
        && opts1.WaveOps.as_bool()
        && (opts1.WaveLaneCountMin..=opts1.WaveLaneCountMax).contains(&lanes)
}

/// Every active lane of the group takes the arm the wave's mask decides, in
/// groups of the console's two wave widths, all lanes or only some active;
/// the lanes outside keep their zero.
#[test]
fn every_active_lane_takes_the_arm_of_the_waves_vote_on_gpu() {
    let _one = super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; skipped");
        return;
    };
    // (lanes below the count run, the lane whose compare matches)
    let cases = |lanes: u32| -> Vec<(u32, u32)> {
        if lanes == 32 {
            vec![
                (0, 0),
                (1, 0),
                (16, 15),
                (16, 16),
                (24, 7),
                (24, 21),
                (24, 25),
                (32, 31),
                (32, 32),
            ]
        } else {
            // 48 active: lane 37 matches, or lane 49 or 60 (outside the
            // running lanes), or the last running lane.
            vec![
                (0, 0),
                (1, 0),
                (32, 31),
                (32, 32),
                (48, 7),
                (48, 37),
                (48, 47),
                (48, 49),
                (48, 60),
                (64, 63),
                (64, 64),
            ]
        }
    };
    let mut ran = 0;
    for lanes in [32, 64] {
        if !runs_waves_of(&host, lanes) {
            eprintln!("host has no {lanes}-lane waves; lowered and validated only");
            continue;
        }
        for kind in KINDS {
            let shader = emit(kind, lanes);
            // WARP (1.0.22) now and then compiles a divergent wave vote
            // into a pipeline that gives the wrong lanes (so does a plain
            // DXC `WaveActiveAnyTrue` in `if (lane < below)`, in a few
            // runs of 25, always the same pipeline): a wrong lowering is
            // wrong in every pipeline, so it takes three wrong ones.
            let mut wrong = Vec::new();
            for _ in 0..3 {
                let (root, pso) = host.compute_pipeline(&shader).expect("compute_pipeline");
                wrong.clear();
                for (below, selected) in cases(lanes) {
                    let mut data = vec![0; shader.user_data_dwords().max(1) as usize];
                    for u in &shader.user_data {
                        if u.root == RootType::Constants && u.slot == 0 && u.dword < 2 {
                            data[u.index as usize] = [below, selected][u.dword as usize];
                        }
                    }
                    let want: Vec<u32> = (0..lanes)
                        .map(|l| {
                            if l < below {
                                1 + u32::from(selected < below)
                            } else {
                                0
                            }
                        })
                        .collect();
                    let got = host
                        .dispatch(
                            &root,
                            &pso,
                            &Dispatch {
                                roots: vec![(0, Root::Constants(data))],
                                views: &[View::RawUav(lanes)],
                                samplers: &[],
                                groups: [1, 1, 1],
                            },
                        )
                        .expect("dispatch")
                        .remove(0);
                    ran += 1;
                    if got != want {
                        wrong.push(format!(
                            "{kind:?}, {lanes} lanes, {below} active, lane {selected}: {got:?}, not {want:?}"
                        ));
                    }
                }
                if wrong.is_empty() {
                    break;
                }
            }
            assert!(
                wrong.is_empty(),
                "{}",
                wrong.join(
                    "
"
                )
            );
        }
    }
    eprintln!("{ran} dispatches");
}
