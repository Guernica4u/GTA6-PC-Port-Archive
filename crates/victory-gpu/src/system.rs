//! What this machine is, asked without a title (#104).
//!
//! A test report says little without the facts the run met: the width of the
//! waves decides which console compute programs can be translated at all
//! (#9), the size of a host view heap decides whether the title's own heap
//! fits (#15, #74), mesh shaders and shader model 6.6 decide which passes can
//! be made, and the commit space decides whether the title gets the 43 GiB it
//! asks for (Windows refuses with error 1455 below that). None of these needs
//! a game file or a running title, so `victory system` can print them.
//!
//! This module holds the facts and what they mean; [`gather`] (Windows only)
//! asks the machine for them. Everything but `gather` is a plain function of
//! the facts, so what a report says can be tested without a card.

use std::fmt;

#[cfg(windows)]
mod gather;

/// Bytes in a gibibyte.
const GIB: f64 = (1u64 << 30) as f64;

/// `D3D_SHADER_MODEL_6_0`, the value the later models count up from.
const SHADER_MODEL_6_0: u32 = 0x60;

/// The major and minor a `D3D_SHADER_MODEL` stands for.
///
/// `D3D12_FEATURE_DATA_SHADER_MODEL` answers with a value of that enum, not
/// with the number of the model: 6.0 is `0x60` and 6.7 is `0x67` (103), so a
/// report that took 100 for 6.0 called 6.7 "6.3" and blamed the title's
/// shaders for it.
pub fn shader_model(value: u32) -> (u32, u32) {
    (6, value.saturating_sub(SHADER_MODEL_6_0))
}

/// Views the title's own heap holds: a little over a million in the retail
/// game. A host whose shader-visible heaps take fewer is capped at the
/// descriptor code's `PORTABLE_VIEWS`; see `descriptor/host.rs`.
const TITLE_VIEWS: u32 = 1_000_000;

/// Commit the title asks for while it streams the world in, in bytes. Below
/// this Windows refuses the allocation with error 1455 ("the paging file is
/// too small for this operation to complete"), which reads as a crash.
const TITLE_COMMIT: u64 = 43 * (1u64 << 30);

/// The shader model the title's world shaders are written for.
const TITLE_SHADER_MODEL: (u32, u32) = (6, 6);

/// What the machine is.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    /// The version of this build (`0.1.0`).
    pub version: String,
    /// The commit it was built from, when the build knew it.
    pub commit: Option<String>,
    /// Whether this is a debug build.
    pub debug: bool,
    /// The Windows version, as this machine reports it.
    pub windows: String,
    pub cpu: Cpu,
    pub memory: Memory,
    pub gpus: Vec<Gpu>,
    /// Which of [`Facts::gpus`] a run would use.
    pub used: Option<usize>,
    /// Whether this is Wine rather than Windows.
    pub wine: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Cpu {
    /// The name the processor gives itself, e.g. "13th Gen Intel(R) Core(TM) i9-13900K".
    pub name: String,
    /// Logical processors.
    pub threads: u32,
    /// Whether AVX2 is available (`zlib-rs` and the translator's
    /// dword-at-a-time copies use it).
    pub avx2: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Memory {
    /// Physical memory.
    pub total: u64,
    /// Physical memory free now.
    pub free: u64,
    /// Commit limit (physical memory plus the paging file).
    pub commit_limit: u64,
    /// Commit free now.
    pub commit_free: u64,
}

/// One adapter, with what a device made on it says it can do.
#[derive(Debug, Default, Clone)]
pub struct Gpu {
    pub name: String,
    pub vendor: u32,
    pub device: u32,
    /// The driver's version, as the adapter reports it: four 16-bit fields.
    pub driver: Option<[u16; 4]>,
    pub dedicated: u64,
    pub shared: u64,
    /// The highest shader model a device on this adapter takes.
    pub shader_model: Option<(u32, u32)>,
    pub mesh_shaders: bool,
    pub wave_min: u32,
    pub wave_max: u32,
    /// The resource binding tier (1, 2 or 3).
    pub binding_tier: u32,
    /// The most views a shader-visible heap this adapter makes holds.
    pub view_heap: u32,
    /// Whether it takes a GPU upload heap (shader model 6.8).
    pub upload_heap: bool,
}

impl Facts {
    /// Why a run on this machine would not go well, in the title's terms.
    ///
    /// Not every imperfection is a blocker: a host without a GPU upload heap
    /// runs the title, only slower (`VICTORY_CARD_BUFFERS_MIN_KB` is the
    /// experiment); a host with waves of 32 lanes keeps the console's wave64
    /// compute programs only where the translator can split them.
    #[must_use]
    pub fn blockers(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.wine {
            out.push("Wine: the title's own anti-tamper ends the run (#6, #19); use Windows in a virtual machine".to_owned());
        }
        let Some(gpu) = self.used.and_then(|i| self.gpus.get(i)) else {
            out.push("No adapter makes a device of feature level 12.0".to_owned());
            return out;
        };
        if gpu.wave_max < 64 {
            out.push(format!(
                "Waves of {} to {} lanes: console wave64 compute programs are dropped where their cross-lane code \
                 cannot be split (#9)",
                gpu.wave_min, gpu.wave_max
            ));
        }
        if gpu.view_heap < TITLE_VIEWS {
            out.push(format!(
                "Shader-visible heaps of {} views: the title's heap has a little over {TITLE_VIEWS} (#15, #74)",
                gpu.view_heap
            ));
        }
        if !gpu.mesh_shaders {
            out.push(
                "No mesh shaders (shader model 6.6): the passes that use them are dropped"
                    .to_owned(),
            );
        }
        if let Some(model) = gpu.shader_model
            && model < TITLE_SHADER_MODEL
        {
            out.push(format!(
                "Shader model {}.{}: the title's shaders are written for {}.{} and above",
                model.0, model.1, TITLE_SHADER_MODEL.0, TITLE_SHADER_MODEL.1
            ));
        }
        if self.memory.commit_free < TITLE_COMMIT {
            out.push(format!(
                "{} GiB of commit free: the title asks for about {} GiB and Windows refuses 1455 below that",
                gib(self.memory.commit_free),
                TITLE_COMMIT / (1u64 << 30)
            ));
        }
        out
    }
}

impl fmt::Display for Facts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.commit {
            Some(commit) => writeln!(
                f,
                "VIctory {} ({commit}), {} build",
                self.version,
                if self.debug { "debug" } else { "release" }
            )?,
            None => writeln!(
                f,
                "VIctory {}, {} build",
                self.version,
                if self.debug { "debug" } else { "release" }
            )?,
        }
        writeln!(f, "{}", self.windows)?;
        writeln!(
            f,
            "CPU: {}, {} threads, AVX2 {}",
            self.cpu.name,
            self.cpu.threads,
            yes(self.cpu.avx2)
        )?;
        writeln!(
            f,
            "Memory: {} GiB RAM ({} GiB free), commit {} GiB free of {} GiB",
            gib(self.memory.total),
            gib(self.memory.free),
            gib(self.memory.commit_free),
            gib(self.memory.commit_limit)
        )?;
        for (i, gpu) in self.gpus.iter().enumerate() {
            writeln!(
                f,
                "GPU{}: {} [{:04x}:{:04x}], driver {}, {} MiB dedicated, {} MiB shared",
                if self.used == Some(i) { " (used)" } else { "" },
                gpu.name,
                gpu.vendor,
                gpu.device,
                gpu.driver.map_or_else(
                    || "unknown".to_owned(),
                    |d| format!("{}.{}.{}.{}", d[0], d[1], d[2], d[3])
                ),
                gpu.dedicated / (1 << 20),
                gpu.shared / (1 << 20)
            )?;
            writeln!(
                f,
                "    shader model {}, mesh shaders {}, waves of {} to {} lanes, resource binding tier {}, {}, GPU upload heap {}",
                gpu.shader_model
                    .map_or_else(|| "unknown".to_owned(), |(a, b)| format!("{a}.{b}")),
                yes(gpu.mesh_shaders),
                gpu.wave_min,
                gpu.wave_max,
                gpu.binding_tier,
                if gpu.view_heap == 0 {
                    "no shader-visible view heap".to_owned()
                } else if gpu.view_heap < TITLE_VIEWS {
                    format!(
                        "view heap of {} views, too small for the title's",
                        gpu.view_heap
                    )
                } else {
                    format!(
                        "view heap of {} views, enough for the title's",
                        gpu.view_heap
                    )
                },
                yes(gpu.upload_heap)
            )?;
        }
        let blockers = self.blockers();
        if blockers.is_empty() {
            writeln!(f, "No known blockers.")?;
        } else {
            writeln!(f, "Known blockers:")?;
            for blocker in blockers {
                writeln!(f, "- {blocker}")?;
            }
        }
        Ok(())
    }
}

/// A byte count in GiB, to one decimal.
fn gib(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / GIB)
}

fn yes(what: bool) -> &'static str {
    if what { "yes" } else { "no" }
}

/// Asks the machine for its facts.
///
/// Nothing here needs a title, a game file or a window: the adapters are
/// enumerated and a device made on each, as `host` would make one.
#[must_use]
pub fn gather() -> Facts {
    #[cfg(windows)]
    {
        gather::gather()
    }
    #[cfg(not(windows))]
    {
        Facts::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> Facts {
        Facts {
            version: "0.1.0".to_owned(),
            commit: Some("7a0ccd3".to_owned()),
            debug: false,
            windows: "Windows 11, build 26100".to_owned(),
            cpu: Cpu {
                name: "13th Gen Intel(R) Core(TM) i9-13900K".to_owned(),
                threads: 32,
                avx2: true,
            },
            memory: Memory {
                total: 64 << 30,
                free: 44 << 30,
                commit_limit: 68 << 30,
                commit_free: 60 << 30,
            },
            gpus: vec![Gpu {
                name: "AMD Radeon RX 7900 XTX".to_owned(),
                vendor: 0x1002,
                device: 0x744c,
                driver: Some([32, 0, 32015, 2008]),
                dedicated: 24 << 30,
                shared: 32 << 30,
                shader_model: Some((6, 8)),
                mesh_shaders: true,
                wave_min: 32,
                wave_max: 64,
                binding_tier: 3,
                view_heap: 4 << 20,
                upload_heap: true,
            }],
            used: Some(0),
            wine: false,
        }
    }

    #[test]
    fn a_shader_model_value_is_read_as_the_model_it_names() {
        assert_eq!(shader_model(0x60), (6, 0));
        assert_eq!(shader_model(0x63), (6, 3));
        assert_eq!(shader_model(0x67), (6, 7));
        assert_eq!(shader_model(0x69), (6, 9));
        // Below 6.0 there is no minor to name, and no model this build knows.
        assert_eq!(shader_model(0x50), (6, 0));
    }

    #[test]
    fn a_machine_that_runs_the_title_has_no_blockers() {
        let facts = machine();
        assert!(facts.blockers().is_empty(), "{:?}", facts.blockers());
        let text = facts.to_string();
        assert!(
            text.contains("VIctory 0.1.0 (7a0ccd3), release build"),
            "{text}"
        );
        assert!(
            text.contains("GPU (used): AMD Radeon RX 7900 XTX [1002:744c]"),
            "{text}"
        );
        assert!(text.contains("waves of 32 to 64 lanes"), "{text}");
        assert!(text.contains("No known blockers."), "{text}");
    }

    #[test]
    fn a_build_that_does_not_know_its_commit_says_so_by_saying_nothing() {
        let mut facts = machine();
        facts.commit = None;
        facts.debug = true;
        assert!(facts.to_string().starts_with("VIctory 0.1.0, debug build"));
    }

    #[test]
    fn waves_of_32_lanes_are_a_blocker_because_wave64_compute_is_dropped() {
        let mut facts = machine();
        facts.gpus[0].wave_min = 32;
        facts.gpus[0].wave_max = 32;
        let blockers = facts.blockers();
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert!(
            blockers[0].contains("Waves of 32 to 32 lanes"),
            "{blockers:?}"
        );
        assert!(blockers[0].contains("#9"), "{blockers:?}");
        assert!(facts.to_string().contains("Known blockers:"), "{facts}");
    }

    #[test]
    fn a_view_heap_under_the_titles_size_is_a_blocker() {
        let mut facts = machine();
        facts.gpus[0].view_heap = 1_000_000;
        assert!(facts.blockers().is_empty(), "exactly the title's size fits");
        facts.gpus[0].view_heap = 500_000;
        let blockers = facts.blockers();
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert!(blockers[0].contains("500000 views"), "{blockers:?}");
    }

    #[test]
    fn a_host_without_mesh_shaders_or_old_shader_model_is_a_blocker() {
        let mut facts = machine();
        facts.gpus[0].mesh_shaders = false;
        facts.gpus[0].shader_model = Some((6, 5));
        let blockers = facts.blockers();
        assert_eq!(blockers.len(), 2, "{blockers:?}");
        assert!(
            blockers.iter().any(|b| b.contains("No mesh shaders")),
            "{blockers:?}"
        );
        assert!(
            blockers.iter().any(|b| b.contains("Shader model 6.5")),
            "{blockers:?}"
        );
    }

    #[test]
    fn a_machine_with_little_commit_is_a_blocker_named_after_1455() {
        let mut facts = machine();
        facts.memory.commit_free = 8 << 30;
        let blockers = facts.blockers();
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert!(
            blockers[0].contains("8.0 GiB of commit free"),
            "{blockers:?}"
        );
        assert!(blockers[0].contains("1455"), "{blockers:?}");
    }

    #[test]
    fn wine_is_a_blocker_of_its_own() {
        let mut facts = machine();
        facts.wine = true;
        let blockers = facts.blockers();
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert!(blockers[0].starts_with("Wine:"), "{blockers:?}");
    }

    #[test]
    fn an_adapter_that_makes_no_device_is_named_as_such() {
        let mut facts = machine();
        facts.used = None;
        let blockers = facts.blockers();
        assert_eq!(blockers.len(), 1, "{blockers:?}");
        assert!(blockers[0].contains("feature level 12.0"), "{blockers:?}");
    }

    #[test]
    fn every_adapter_is_printed_and_only_the_used_one_is_marked() {
        let mut facts = machine();
        let mut second = facts.gpus[0].clone();
        second.name = "Intel(R) UHD Graphics".to_owned();
        second.vendor = 0x8086;
        second.device = 0x46a6;
        second.view_heap = 0;
        facts.gpus.push(second);
        facts.used = Some(1);
        let text = facts.to_string();
        assert!(text.contains("GPU: AMD Radeon RX 7900 XTX"), "{text}");
        assert!(text.contains("GPU (used): Intel(R) UHD Graphics"), "{text}");
        assert!(text.contains("no shader-visible view heap"), "{text}");
    }
}
