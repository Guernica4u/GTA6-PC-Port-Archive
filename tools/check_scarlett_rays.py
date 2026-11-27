#!/usr/bin/env python3
"""Replay ProjectScarlett's shadow ray programs with native ray queries.

The programs come from the recorded shader requests of a ProjectScarlett run
(see tools/triage_scarlett_shaders.py). The provider translates each one,
replacing its inlined console BVH walk with a native ray query against the
scene bound in its private space, and the guest compares every lane with the
geometry it built.
"""
import argparse
import json
import os
import shutil
from pathlib import Path
import sys
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from build import Toolchain
from project_tuxbox.host import Host
from project_tuxbox.inspect import digest
from project_tuxbox.runtime_shaders import runtime_shader_compiler

SOURCE = ROOT / "tools/reference/scarlett"
# (request hash prefix, walk). The laser and sun programs are recorded without
# a root signature; the guest declares one naming the registers they read.
PROGRAMS = {
    "CS_IrradianceCache_ShadowTrace": ("03092f6759cdcd61", "near"),
    "CS_IrradianceCache_ShadowTrace_WithFar": ("2ac21a538145ecc0", "far"),
    "CS_LaserLightShadowRaytracing": ("6247ee64c925b874", "laser"),
    "CS_RaytraceShadows_Mixed": ("641ecfc4956cc4c0", "sun"),
}


def request(directory, prefix):
    found = sorted(directory.glob(f"{prefix}*.dxbc"))
    if len(found) != 1:
        raise ValueError(f"expected one request {prefix}* in {directory}, found {len(found)}")
    return found[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ("requests", "xshader", "provider", "proton", "prefix", "output"):
        parser.add_argument(f"--{option}", required=True, type=Path)
    args = parser.parse_args()
    report = {"provider_sha256": digest(args.provider), "programs": {}}
    (ROOT / "build").mkdir(exist_ok=True)
    with TemporaryDirectory(dir=ROOT / "build") as tmp:
        work = Path(tmp)
        os.environ["TUXBOX_XSHADER"] = str(args.xshader.resolve())
        shutil.copyfile(args.provider, work / "d3d12_xs.dll")
        toolchain = Toolchain()
        obj = toolchain.compile(SOURCE / "rays_guest.cpp", work, "/W4", "/WX", f"/I{ROOT / 'runtime/d3d12'}")
        executable = work / "scarlett-rays.exe"
        toolchain.link(executable, obj, "d3d12.lib", "dxgi.lib", "kernel32.lib")
        host = Host(None, args.proton, args.prefix)
        failed = []
        for name, (prefix, walk) in PROGRAMS.items():
            source = request(args.requests, prefix)
            shutil.copyfile(source, work / f"{prefix}.dxbc")
            receipt = work / f"{prefix}.txt"
            with runtime_shader_compiler("proton", host.env, executable, lambda _: None):
                result = host.run(executable, [f"{prefix}.dxbc", walk, receipt.name], capture=True, timeout=300)
            if not receipt.exists():
                raise RuntimeError(f"{name} guest exited {result.returncode}: {result.stdout[-1500:]!r}")
            lines = receipt.read_text().splitlines()
            print(f"{name}:", *lines, sep="\n  ")
            fields = dict(line.split("=", 1) for line in lines if "=" in line)
            report["programs"][name] = {"source_sha256": digest(source), "returncode": result.returncode, **fields}
            if result.returncode or fields.get("failed") != "0":
                failed.append(name)
                output = (result.stdout or b"").decode(errors="replace")
                print(output[-3000:])
    report["host"] = host.version
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    if failed:
        raise SystemExit(f"Scarlett ray replay failed: {', '.join(failed)}")


if __name__ == "__main__":
    main()
