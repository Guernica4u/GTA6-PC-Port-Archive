# Installation — VIctory Fork (ors33/victory-fork)

Source-only. No game files are included; supply your own dump of the title's
Xbox Series build v1.0.2.1 (folder with `game_scarlett_master.exe`).

## Prerequisites

- Windows 10/11 x64, AVX2 CPU, 32 GB RAM+, SSD
- NVIDIA GeForce RTX, recent driver, HAGS on
- Rust 1.98.1 (rustup) + Visual Studio Build Tools "Desktop development with C++"

## Get source

```powershell
git clone https://github.com/ors33/victory-fork
cd victory-fork
```

## Configure

```powershell
copy victory.example.toml victory.toml
```

Set `game_dir` in `victory.toml` to your dump folder (outside the repo).

## DLSS files (optional, ~70 MB)

```powershell
.\Get-NVIDIA-DLSS.cmd
```

Or later: `victory get-dlss`. Without them, runs with own upscaling, no frame gen.

## Build

```powershell
cargo build --release -p victory
```

Diagnostics build:

```powershell
cargo build --profile diag -p victory --features diagnostics
```

## Git hooks (contributors)

```powershell
cargo xtask install-hooks
```

## Run

Place `victory.exe` beside `game_scarlett_master.exe` in your dump folder:

```powershell
victory play
```

Or:

```powershell
victory play 'D:\Games\MyDump\game_scarlett_master.exe'
```

Options: `-SeriesS`, `-FrameGen`/`-FrameGenX 4`, `-Dlss off|dlaa|balanced|performance`,
`-Fps 60`, `-Res 1080to1440|4k`, `-Build diag`.

F1 menu saves override options via `victory-settings.txt`; delete it to reset.

## In game

- F1: settings menu. F3: stats panel.
- Full screen: run `.\scripts\fullscreen.ps1` after starting.

Logs: `%LOCALAPPDATA%\victory\logs`. Saves: `%LOCALAPPDATA%\victory\data`.

## Verify (contributors)

```powershell
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo xtask policy
```

## Troubleshooting

- First build stutters while pipelines compile; run `.\scripts\warm_shaders.bat` once.
- "SAVE AND RESTART NOW" may leave old process; restarts clear it.
- Frame gen needs VRAM; launcher holds back 1/5 of budget.
- Linux/Proton: use `launch_linux_proton.sh`.

## References

- README.md, docs/architecture.md, docs/asset-policy.md, playability-resume.md
- Issues: https://github.com/ors33/victory-fork/issues

## Original upstream install (VIctory / VIctory-NVIDIA)

1. Get your own dump of the title (Xbox Series v1.0.2.1).
2. Install Rust + Visual Studio Build Tools "Desktop development with C++".
3. PowerShell:
   ```powershell
   git clone https://github.com/trustno1-coder/VIctory-NVIDIA
   cd VIctory-NVIDIA
   copy victory.example.toml victory.toml
   .\Get-NVIDIA-DLSS.cmd
   scripts\play-nvidia.ps1 -Exe 'D:\Games\MyDump\game_scarlett_master.exe'
   ```
4. First start builds the layer.
5. Requirements: Windows 10/11, AVX2, 32 GB RAM+, SSD, NVIDIA RTX.
6. In game: F1 settings, F3 stats, `.\scripts\fullscreen.ps1`.

Upstream: https://github.com/trustno1-coder/VIctory-NVIDIA
Original: https://github.com/Marvisak/VIctory
This fork: https://github.com/ors33/victory-fork