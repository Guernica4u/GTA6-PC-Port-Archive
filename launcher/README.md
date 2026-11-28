# GTA VI Launcher

A small Windows launcher for the PC port runtimes in this archive. Point it at your own copy of the game's `.exe` once, pick a runtime and press **PLAY**.

Tabs on the left side:

- **Launcher:** the slideshow, your game, the runtime in use (its name and whether it's made for NVIDIA or AMD cards), and PLAY.
- **Conversion:** every runtime, with its rank, whether it's made for NVIDIA or AMD cards, how far it gets and whether it's installed. Click one to use it.
- **Settings:** the game path, the song, mute, and the pictures folder.

**[Download GTA-VI-Launcher.zip](https://github.com/Guernica4u/GTA6-PC-Port-Archive/releases/download/launcher/GTA-VI-Launcher.zip)** (Windows 10/11, about 50 KB)

Unzip it into a folder of its own. The launcher keeps everything next to itself: your settings, the runtimes it builds, their caches and the slideshow pictures.

## What it does

- **Runtimes:** the archive's top 3, read live from [the archive site](https://guernica4u.github.io/GTA6-PC-Port-Archive/), plus Kelvaroq, VIctory-NVIDIA, Belobug, 7900XTX, ztrx, Hyphen (hyphenn), cool_fox :CGBJ, VIctory and xcl, under the site's names. ztrx, Belobug and 7900XTX were taken off GitHub and cool_fox went private; the launcher installs the copies this archive saved. Hover over a runtime to see how far it gets.
- **Install & play:** the first PLAY on a runtime downloads its source from this archive into `runtimes\<name>` next to the launcher and builds it into that folder's `target\release`. That needs [git](https://git-scm.com/download/win) and [Rust](https://rustup.rs) with the MSVC build tools (Kelvaroq also needs 64-bit [Python](https://www.python.org/downloads/windows/) 3.11 or later). VIctory-NVIDIA uses DLSS only after you run its `Get-NVIDIA-DLSS.cmd` once in `runtimesictory-nvidia`; without it the game uses its own upscaling. cool_fox :CGBJ has a build picker in the Conversion tab, like the site's download picker: its ready-made builds (the newest first) or its source. A ready-made build needs no git or Rust: the launcher downloads its ZIP from the archive's release, unpacks it into `runtimes\cool-fox-<release>\target\release` and runs it only if the exe matches the checksum in the release notes. The exes aren't signed, and Windows Defender flags the pareto-residency ZIP. A console window shows the progress and stays open if something fails.
- **Updates:** when a new launcher release is out, a notice at the top says so; click it to open the download page and unzip the new one over the old folder. When a runtime you installed from source gets new code in the archive (README, docs and image changes don't count), the Conversion tab says "Update available" and PLAY turns into **UPDATE**: it pulls the new code into the same folder and rebuilds it. You can also say No and play the version you have.
- **Slideshow:** shows every picture in the `backgrounds` folder next to the launcher. To add pictures, drag them onto the window. With no pictures, it shows the game's own splash screen.
- **Music:** plays a song from your copy of the game's `audio.rpf`, using the Opus decoder built into Windows. Settings switches between 5 songs. The speaker button in the top bar mutes it. Music stops while the game runs. Only the v1.0.2.1 dump is supported: the songs are found by their position in its `audio.rpf`.
- **Full screen:** the maximize button, F11, Alt+Enter, or double-click the window. Esc leaves full screen.

No game files are included or downloaded. The launcher only reads your own copy.

## Build from source

`build.bat` compiles `Launcher.cs` with the C# compiler that ships with Windows. Nothing else is needed. `GTA VI Launcher.exe --selftest` runs its checks. `make-icon.ps1` redraws `icon.ico`.
