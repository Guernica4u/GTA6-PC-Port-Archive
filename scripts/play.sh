#!/usr/bin/env bash
# Linux: starts the title under Proton's Wine with the settings of scripts/play.ps1.
#   scripts/play.sh <path of your own copy's executable> [--safe] [--ray-tracing] [--build diag|release]
#                   [--seconds N] [--mem-max 26G] [VAR=value ...]
# --safe          without the newer, faster ways (the first thing to try when something is wrong)
# --ray-tracing   the title's own ray-traced indirect light
# --seconds N     ends the run after N seconds (test runs)
# --mem-max SIZE  runs in a systemd scope that may hold SIZE of RAM at most: a run that grows past it
#                 is ended by the kernel instead of freezing the desktop
# VAR=value       other settings, after the ones below: VICTORY_PANEL=1
#
# What it expects (none of it is in this repository; docs/proton.md says how each is made):
#   PROTON_FILES        a Proton's `files` folder (default: proton-cachyos-slr)
#   VICTORY_LINUX_HOME  a folder of ours (default: ~/.local/share/victory-linux) with
#     pfx/              a Wine prefix made by that Proton's `wine wineboot -u`, holding in system32
#                       vkd3d-proton's d3d12.dll and d3d12core.dll, DXVK's dxgi.dll and Microsoft's
#                       d3dcompiler_47.dll
#     bin/wineserver    optional: a wineserver of the same Wine that keeps views in a tree
#                       (Wine bug 60868); it is started before the title when it is there
# The executable must lie beside the package's files (its folder is the title's G:).
set -euo pipefail

# Paths are the caller's: made whole here, before the folder changes below. (Links are kept as
# they are given: the folder the executable is named in is the title's G:.)
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
home=$(realpath -ms "${VICTORY_LINUX_HOME:-$HOME/.local/share/victory-linux}")
proton=$(realpath -ms "${PROTON_FILES:-/usr/share/steam/compatibilitytools.d/proton-cachyos-slr/files}")

exe='' build=diag safe=0 rays=0 seconds=0 mem_max='' more=()
while (($#)); do
    case $1 in
        --safe) safe=1 ;;
        --ray-tracing) rays=1 ;;
        --build) build=$2; shift ;;
        --seconds) seconds=$2; shift ;;
        --mem-max) mem_max=$2; shift ;;
        *=*) more+=("$1") ;;
        *) exe=$1 ;;
    esac
    shift
done

victory=$root/target/x86_64-pc-windows-msvc/$build/victory.exe
[[ -f $victory ]] || { echo "No $victory. Build it first: cargo xwin build --locked --profile diag -p victory --features diagnostics --target x86_64-pc-windows-msvc"; exit 1; }
[[ -f $exe ]] || { echo "No executable '$exe'."; exit 1; }
exe=$(realpath -s "$exe")
[[ -x $proton/bin/wine ]] || { echo "No Wine in $proton (PROTON_FILES)."; exit 1; }
[[ -d $home/pfx/drive_c ]] || { echo "No prefix in $home/pfx (see the head of this script)."; exit 1; }

# Other settings left over in this shell go; where your files are stays.
for name in $(compgen -e VICTORY_); do
    case $name in VICTORY_CONFIG | VICTORY_GAME_DIR | VICTORY_CACHE_DIR | VICTORY_DATA_DIR | VICTORY_LINUX_HOME) ;; *) unset "$name" ;; esac
done

# As scripts/play.ps1, but for VICTORY_LAND_ECHO: with it on, heads are drawn without skin under Wine.
settings=(
    VICTORY_TEXTURE_NO_WAIT=1 VICTORY_RELAY=2 VICTORY_WIDE_BACK=1 VICTORY_CARD_REHASH_RUNS=1
    VICTORY_JOIN_COPIES=1 VICTORY_INDIRECT_WINDOW=1 VICTORY_RELAY_COPY=1 VICTORY_CARD_STEPS=2
    VICTORY_INDIRECT_BATCH=2 VICTORY_MAP_THREADS=1 VICTORY_TITLE_PRIORITIES=0 VICTORY_CARD_AWAKE=1
    VICTORY_SHOW_AFTER=1 VICTORY_AMBIENT=0.8 VICTORY_D3D12_DRED=light VICTORY_SKIP_PIPELINES=none
    VICTORY_LAND_ECHO=0
)
if ((safe)); then
    settings+=(VICTORY_UNDER_WIDE=0)
else
    settings+=(VICTORY_THIN=1 VICTORY_JOIN_PARTS=8 VICTORY_UPLOAD_AHEAD=1)
fi
((rays)) && settings+=(VICTORY_BVH_BUILDER=1)

windows() { printf 'Z:%s' "${1//\//\\}"; }
mkdir -p "$home"/{cache,data,logs,vkd3d-cache}
log=$home/logs/$(date +%Y%m%d-%H%M%S).log
wine=(
    WINEPREFIX="$home/pfx" WINEDEBUG="${WINEDEBUG:--all}" WINEFSYNC=1
    LD_LIBRARY_PATH="$proton/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    WINEDLLOVERRIDES='d3d12,d3d12core,dxgi,d3dcompiler_47=n;mscoree,mshtml='
    VKD3D_SHADER_CACHE_PATH="$home/vkd3d-cache" VKD3D_CONFIG="${VKD3D_CONFIG-descriptor_heap}"
    VICTORY_CACHE_DIR="${VICTORY_CACHE_DIR:-$(windows "$home/cache")}"
    VICTORY_DATA_DIR="${VICTORY_DATA_DIR:-$(windows "$home/data")}"
)

# Wine uses a server that already runs for the prefix, whichever file it is.
server=$proton/bin/wineserver
[[ -x $home/bin/wineserver ]] && server=$home/bin/wineserver
env "${wine[@]}" "$server" -p 2>/dev/null || true

scope=()
[[ -n $mem_max ]] && scope=(systemd-run --user --scope --quiet --collect -p MemoryMax="$mem_max" -p MemorySwapMax=8G)
if ((seconds)); then
    (sleep "$seconds" && env "${wine[@]}" "$server" -k) 2>/dev/null &
    timer=$!
    trap 'pkill -P $timer 2>/dev/null; kill $timer 2>/dev/null || true' EXIT
fi

echo "log: $log"
cd "$root" # victory.toml is looked for here
# Into a file, not a pipe: Wine's services outlive the title and would hold a pipe open.
: >"$log"
"${scope[@]}" env "${wine[@]}" "${settings[@]}" "${more[@]}" \
    "$proton/bin/wine" "$victory" run "$(windows "$exe")" --execute >"$log" 2>&1 </dev/null &
tail --pid=$! -f "$log"
wait $!
