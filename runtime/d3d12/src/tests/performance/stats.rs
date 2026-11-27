use super::*;

fn frames(times: &[u32]) -> Frames {
    let mut frames = Frames::default();
    times.iter().for_each(|&t| frames.push(t));
    frames
}

#[test]
fn rates_cover_the_last_second_only() {
    // Two slow frames, then a second of 10 ms frames.
    let mut times = vec![500_000, 500_000];
    times.extend([10_000; 100]);
    let frames = frames(&times);
    assert_eq!(frames.fps(), Some(100.0));
    assert_eq!(frames.times(), Some((10.0, 10.0)));
}

#[test]
fn a_frame_longer_than_a_second_is_still_reported() {
    let frames = frames(&[16_000, 2_000_000]);
    assert_eq!(frames.fps(), Some(0.5));
    assert_eq!(frames.times(), Some((2000.0, 2000.0)));
}

#[test]
fn the_low_is_the_rate_of_the_slowest_hundredth() {
    let mut times = vec![10_000; 198];
    times.extend([40_000, 60_000]);
    assert_eq!(frames(&times).low(), Some(20.0));
    assert_eq!(frames(&[25_000]).low(), Some(40.0));
}

#[test]
fn history_is_bounded_and_the_graph_reads_the_newest() {
    let mut frames = Frames::default();
    (0..HISTORY as u32 + 5).for_each(|i| frames.push(i * 1000));
    assert_eq!(frames.0.len(), HISTORY);
    assert_eq!(frames.recent(2).collect::<Vec<_>>(), [(HISTORY + 3) as f32, (HISTORY + 4) as f32]);
}

#[test]
fn nothing_measured_reads_as_missing() {
    let empty = Frames::default();
    assert_eq!((empty.fps(), empty.times(), empty.low()), (None, None, None));
    assert_eq!(share(5, 0), None);
    let lines = lines(&empty, &Host::default(), &Work::default());
    assert_eq!(lines[0], "FPS -   1% low -");
    assert_eq!(lines[7], "VRAM -");
    assert_eq!(lines[3], "GPU -%  - C  - MHz  - W");
}

#[test]
fn lines_show_the_host_and_tuxbox_numbers() {
    let host = Host {
        cpu_system: Some(45.4),
        cpu_game: Some(38.0),
        game_resident: Some(13 << 30),
        game_committed: Some(29 << 30),
        ram: Some((22 << 30, 32 << 30)),
        swap: Some((15 << 30, 62 << 30)),
        video: Some((3 << 29, 15 << 30)),
        gpu: Gpu { load: Some(98.0), celsius: Some(61.4), megahertz: Some(2745.0), watts: None },
    };
    let work = Work { translating: 3, building: 2, empty_pipelines: 195 };
    let lines = lines(&frames(&[20_000; 60]), &host, &work);
    assert_eq!(lines, [
        "FPS 50.0   1% low 50.0",
        "Frame 20.0 ms   longest 20.0 ms",
        "CPU 45%   game 38%",
        "GPU 98%  61 C  2745 MHz  - W",
        "Game 13.0 GB  committed 29.0 GB",
        "RAM 22.0 / 32.0 GB",
        "Swap 15.0 / 62.0 GB",
        "VRAM 1.5 / 15.0 GB",
        "Shaders 3 | building 2 | empty 195",
    ]);
}

#[test]
fn bars_stay_inside_the_graph() {
    assert_eq!(bar(0.0, 48), 1);
    assert_eq!(bar(25.0, 48), 24);
    assert_eq!(bar(5000.0, 48), 48);
}

#[test]
fn sysfs_values_are_read_in_their_units() {
    assert_eq!(pci_id("0x1002\n"), Some(0x1002));
    assert_eq!(pci_id("1002"), None);
    assert_eq!(scaled("45000\n", 1000.0), Some(45.0));
    assert_eq!(scaled("2200000000\n", 1e6), Some(2200.0));
    assert_eq!(scaled("\n", 1.0), None);
}

#[test]
fn only_the_nvml_entry_of_the_wine_dll_path_is_used() {
    assert_eq!(wine_nvml_path("/opt/p/files/lib/wine/nvidia-libs/nvml/wine").as_deref(),
               Some(r"Z:\opt\p\files\lib\wine\nvidia-libs\nvml\wine\x86_64-windows\nvml.dll"));
    assert_eq!(wine_nvml_path("/opt/p/files/lib/wine"), None);
    assert_eq!(wine_nvml_path("nvml/wine"), None);
}
