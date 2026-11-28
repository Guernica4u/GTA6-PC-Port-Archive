# Installation Guide — VIctory Fork (ors33/victory-fork)

This file sits at the repository root so it is easy to find. It explains how to
build and run the layer from source. The layer contains **no game files**; you
must supply your own legally obtained dump of the title's Xbox Series build
v1.0.2.1 (the folder containing `game_scarlett_master.exe`).

This guide is for **our fork**: https://github.com/ors33/victory-fork
(issue tracker: https://github.com/ors33/victory-fork/issues).

## 1. Prerequisites

- Windows 10 or 11 (x64)
- CPU with AVX2 (AMD Zen 2+ or Intel Haswell+)
- 32 GB RAM or more (64 GB comfortable), SSD
- NVIDIA GeForce RTX card with a recent driver (RTX 40+ for frame generation)
- Hardware-accelerated GPU scheduling (HAGS) enabled
- Rust (toolchain pinned in `rust-toolchain.toml`: 1.98.1)
- Visual Studio Build Tools with "Desktop development with C++"

## 2. Get the source

```powershell
git clone https://github.com/ors33/victory-fork
cd victory-fork
```

## 3. Configure

```powershell
copy victory.example.toml victory.toml
```

Edit `victory.toml` and set `game_dir` to the folder of your dump (outside this
repository). Optionally set `cache_dir` (defaults to `%LOCALAPPDATA%\victory\cache`).

## 4. Fetch NVIDIA's DLSS / Frame Generation files (optional, ~70 MB)

```powershell
.\Get-NVIDIA-DLSS.cmd
```

These are NVIDIA's own files, fetched from NVIDIA's GitHub. Without them the game
still runs with its own upscaling and no frame generation. You can also run
`victory get-dlss` later.

## 5. Build

```powershell
cargo build --release -p victory
```

The first build takes a few minutes. For the diagnostics build (captures,
timelines, recording time in the F3 panel):

```powershell
cargo build --profile diag -p victory --features diagnostics
```

## 6. Install git hooks (recommended for contributors)

```powershell
cargo xtask install-hooks
```

This enables the asset-policy check on every commit.

## 7. Run

Put `victory.exe` beside `game_scarlett_master.exe` in your dump folder, then:

```powershell
victory play
```

Or name the executable explicitly:

```powershell
victory play 'D:\Games\MyDump\game_scarlett_master.exe'
```

First start builds the layer (a few minutes). Useful options:

| Option | Effect |
|--------|--------|
| `-SeriesS` | Series S profile (1280x720 inside, faster, softer) |
| `-FrameGen`, `-FrameGenX 4` | Frame generation on from start (2 to 6 frames shown per rendered) |
| `-Dlss off` / `dlaa` / `balanced` / `performance` | DLSS mode |
| `-Fps 60` | Frame rate cap (30 is the title's own) |
| `-Res 1080to1440` / `4k` | Scene at 1080p upscaled / stock 4K output |
| `-Build diag` | Diagnostics build |

What the F1 menu saves (`victory-settings.txt`) overrides these at every start;
delete that file to go back to defaults.

## 8. In the game

- **F1** — settings menu (arrow keys pick/change, Enter applies)
- **F3** — stats panel (frame rate, draws, GPU and memory numbers)
- Full screen: start the game, then run `.\scripts\fullscreen.ps1` in another window

Logs go to `%LOCALAPPDATA%\victory\logs`, saves to `%LOCALAPPDATA%\victory\data`.

## 9. Verify (contributors)

```powershell
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo xtask policy
```

## Troubleshooting

- First run of every new build stutters while the driver compiles pipelines;
  run `.\scripts\warm_shaders.bat` (or `victory warm`) once, then restart.
- "SAVE AND RESTART NOW" can leave the old process behind without a window;
  it goes when the PC restarts.
- Frame generation needs room in the card's memory; the launcher holds back a
  fifth of the budget when frame generation is ready or on.
- Linux/Proton: use `launch_linux_proton.sh` to stop the bundled
  `D3D12Core.dll` from replacing vkd3d-proton.

## See also

- `README.md` — full project overview and status
- `docs/architecture.md` — the layer's parts
- `docs/asset-policy.md` — what may and may not be in this repository
- `playability-resume.md` — what is still needed to make the game playable
- Issue tracker: https://github.com/ors33/victory-fork/issues