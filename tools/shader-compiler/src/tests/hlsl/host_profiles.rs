//! What each host profile receives for a fixed set of synthetic programs.
//!
//! A change to the translator shows here which hosts it affects. When the change
//! is meant, update the record and include it in the pull request:
//! `TUXBOX_BLESS_PROFILES=1 cargo test ... host_profiles`.
use super::*;

#[path = "host_profile_programs.rs"]
mod programs;

const RECORD: &str = include_str!("host_profiles.txt");

fn program(threads: u32, words: &[u32]) -> ShaderExtract {
    let isa: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    ShaderExtract {
        flat_inputs: Vec::new(), flat_outputs: Vec::new(), stage: Stage::Compute, shader_model: (6, 0),
        required_features: None, bitcode_stripped: true, fspx_config: None, ps09_pixel_inputs: None,
        fssm: Vec::new(), entry: Some("main".into()), isa_data: isa.clone(), isa, isa_offset_in_fssm: 0,
        psv_threads: None, isa_preamble: [0, threads, 1, 1], has_input_sig: false, has_output_sig: false,
        has_psv0: false, has_root_sig: false, input_sig: Vec::new(), output_sig: Vec::new(), resources: Vec::new(),
        gs_info: None, mesh_info: None, primitive_sig: Vec::new(), root_sig: None,
    }
}

/// FNV-1a, enough to notice that a translation changed.
fn summary(translated: &Result<Translated, String>) -> String {
    match translated {
        Ok(t) => {
            let hash = t.hlsl.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
            });
            let waves = t.required_wave_size.map_or("any".into(), |width| width.to_string());
            format!("{} waves={waves} {hash:016x}", t.profile)
        }
        Err(_) => "refused".into(),
    }
}

#[test]
fn every_host_profile_receives_the_recorded_translation() {
    let mut now = String::new();
    let mut narrowed = 0;
    for &(name, threads, words) in programs::PROGRAMS {
        let shader = program(threads, words);
        let ordinary = translate(&shader);
        let narrow = translate_for_wave32_host(&shader);
        // Only a program that needs 64-lane waves may differ on a narrower host.
        if matches!(&ordinary, Ok(t) if t.required_wave_size == Some(64)) {
            narrowed += 1;
        } else {
            assert_eq!(summary(&narrow), summary(&ordinary), "{name}");
        }
        writeln!(now, "{name} any-host {}", summary(&ordinary)).unwrap();
        writeln!(now, "{name} wave32-host {}", summary(&narrow)).unwrap();
        writeln!(now, "{name} two-lanes {}", summary(&translate_paired_wave64(&shader))).unwrap();
    }
    assert!(narrowed > 0, "no program exercises the narrow-wave path");
    if std::env::var_os("TUXBOX_BLESS_PROFILES").is_some() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/hlsl/host_profiles.txt");
        std::fs::write(path, &now).unwrap();
        return;
    }
    let recorded = RECORD.replace("\r\n", "\n");
    let changed: Vec<&str> = now.lines().filter(|line| !recorded.lines().any(|old| old == *line)).collect();
    assert!(
        now == recorded,
        "translations differ for these programs and host profiles:\n{}\nIf this is intended, rerun with \
         TUXBOX_BLESS_PROFILES=1 and commit host_profiles.txt.",
        changed.join("\n")
    );
}


#[test]
fn paired_local_exchanges_need_only_one_read_per_guest_lane() {
    // All four instruction families select a source inside the lane's own
    // 32-lane half. Each of these fixtures has exactly two exchange operations.
    for name in ["dpp8", "row_shift", "row_masks", "permlane"] {
        let (_, threads, words) = programs::PROGRAMS.iter().find(|p| p.0 == name).unwrap();
        let hlsl = translate_paired_wave64(&program(*threads, words)).unwrap().hlsl;
        assert_eq!(hlsl.matches("WaveReadLaneAt(").count(), 4, "{name}");
        assert!(!hlsl.contains("tb_low"), "{name}");
        assert!(!hlsl.contains("tb_high"), "{name}");
    }
}
