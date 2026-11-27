#!/usr/bin/env python3
"""Report translated bindings a request's own root signature cannot reach.

vkd3d-proton converts a translated program through dxil-spirv, which maps every
resource in the DXIL onto the pipeline's root signature. A binding outside every
table range, root descriptor or the directly-indexed heaps cannot be mapped, and
the pipeline is refused with "dxil-spirv: Failed to remap SRV <space>:<register>".
The provider then substitutes an empty pipeline.

This tool translates recorded requests with the built translator, reads the root
signature each request carries in its RTS0 chunk, and reports the resources a
translated program declares and uses that the signature does not describe. It
needs no GPU and changes nothing: it neither writes the runtime cache nor starts
a game.

A declaration counts as used when the emitted HLSL reads it, including through
the byte-accessor helpers the translator emits for a buffer. That over-counts a
declaration whose helper is emitted but never called, which errs towards
reporting coverage gaps rather than hiding them.

Usage:
    python tools/report_root_coverage.py REQUESTS [--xshader PATH] [--limit N]
    python tools/report_root_coverage.py --failures RUN.tsv --capture DIR

The second form joins a run's pipeline-failure report to the stages captured
during it (TUXBOX_CAPTURE_RASTER_ROOTS / TUXBOX_CAPTURE_SHADER_INPUTS) and says,
for each failing stage, which binding its own pipeline's signature cannot reach
and why.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import re
import struct
import subprocess
from collections import Counter
from pathlib import Path

HEAP_INDEXED = 0x400
SAMPLER_HEAP_INDEXED = 0x800
# D3D12_DESCRIPTOR_RANGE_TYPE: the serialized root uses SRV 0, UAV 1, CBV 2,
# sampler 3. runtime/dxbc's Binding kind uses the same numbers.
RANGE_TYPES = {0: "srv", 1: "uav", 2: "cbv", 3: "sampler"}
PARAM_TYPES = {0: "table", 1: "constants", 2: "cbv", 3: "srv", 4: "uav"}
REGISTER_KINDS = {"t": "srv", "b": "cbv", "u": "uav", "s": "sampler"}
DEFAULT_XSHADER = Path("build/shader-compiler/x86_64-unknown-linux-gnu/release/xshader")

# The emitted declarations are one per line, and a generic argument list may
# contain commas and nested brackets, so match the type name before the
# variable rather than trying to parse the template.
DECLARATION = re.compile(
    r"^\s*(StructuredBuffer|RWStructuredBuffer|ByteAddressBuffer|RWByteAddressBuffer|Buffer|"
    r"RWTexture\w*|Texture\w*|cbuffer|SamplerState|SamplerComparisonState)(?:<.*>)?\s+(\w+)[^;\n]*?"
    r"register\(\s*([tbusr])(\d+)(?:\s*,\s*space(\d+))?\s*\)",
    re.M,
)


def rts0_payload(data):
    """The RTS0 chunk of a DXBC container, or None when the request has none."""
    if data[:4] != b"DXBC" or len(data) < 32:
        return None
    count = struct.unpack_from("<I", data, 28)[0]
    for index in range(count):
        at = 32 + 4 * index
        if at + 4 > len(data):
            return None
        offset = struct.unpack_from("<I", data, at)[0]
        if data[offset:offset + 4] != b"RTS0":
            continue
        size = struct.unpack_from("<I", data, offset + 4)[0]
        payload = data[offset + 8:offset + 8 + size]
        return payload if len(payload) == size else None
    return None


def parse_root(payload):
    """Return (version, flags, params) for a serialized root signature.

    Layout follows runtime/dxbc/src/rootsig.rs. Each parameter is
    (visibility, kind, detail), where a table's detail is a list of
    (range kind, count, base register, space, offset in table).
    """
    version, num_params, params_offset, _, _, flags = struct.unpack_from("<6I", payload, 0)
    if version not in (1, 2):
        raise ValueError(f"unsupported RTS0 version {version}")
    v11 = version == 2
    params = []
    for index in range(num_params):
        tag, visibility, offset = struct.unpack_from("<3I", payload, params_offset + 12 * index)
        if tag == 0:
            count, ranges_offset = struct.unpack_from("<2I", payload, offset)
            stride = 24 if v11 else 20
            ranges = []
            for row in range(count):
                at = ranges_offset + row * stride
                kind, num, base, space = struct.unpack_from("<4I", payload, at)
                ranges.append((RANGE_TYPES.get(kind, kind), num, base, space))
            params.append((visibility, "table", ranges))
        elif tag == 1:
            register, space, num32 = struct.unpack_from("<3I", payload, offset)
            params.append((visibility, "constants", (register, space, num32)))
        elif tag in PARAM_TYPES:
            register, space = struct.unpack_from("<2I", payload, offset)[:2]
            params.append((visibility, PARAM_TYPES[tag], (register, space)))
        else:
            raise ValueError(f"unknown RTS0 parameter type {tag}")
    return version, flags, params


def covers(params, flags, kind, space, register):
    """How a binding is reached, or None when the signature does not describe it."""
    if flags & (SAMPLER_HEAP_INDEXED if kind == "sampler" else HEAP_INDEXED):
        return "heap"
    for _, param_kind, detail in params:
        if param_kind == "table":
            for range_kind, num, base, range_space in detail:
                if range_kind == kind and range_space == space and base <= register < base + num:
                    return "table"
        elif param_kind == kind and detail[:2] == (register, space):
            return "descriptor"
        elif param_kind == "constants" and kind == "cbv" and detail[:2] == (register, space):
            # The shader sees root constants as a constant buffer at that slot.
            return "constants"
    return None


def explain(params, flags, kind, space, register):
    """Why a binding has no element in the signature, in a few words."""
    same_space = [
        (base, num) for _, param_kind, detail in params if param_kind == "table"
        for range_kind, num, base, range_space in detail
        if range_kind == kind and range_space == space
    ]
    other_kind = [
        range_kind for _, param_kind, detail in params if param_kind == "table"
        for range_kind, num, base, range_space in detail
        if range_kind != kind and range_space == space and base <= register < base + num
    ]
    if flags & (SAMPLER_HEAP_INDEXED if kind == "sampler" else HEAP_INDEXED):
        return "directly indexed heap"
    if other_kind:
        return f"space {space} binds {other_kind[0]} at {register}, not {kind}"
    if not same_space:
        return f"no {kind} range in space {space}"
    nearest = min(same_space, key=lambda row: min(abs(row[0] - register), abs(row[0] + row[1] - register)))
    return f"nearest {kind} range in space {space}: {nearest[0]}..{nearest[0] + nearest[1] - 1}"


def declarations(hlsl):
    """The resources an emitted program declares and reads, as (kind, space, register, name)."""
    found = []
    for _, name, letter, number, space in DECLARATION.findall(hlsl):
        uses = len(re.findall(rf"\b{re.escape(name)}(?:\.|\[)", hlsl))
        if uses:
            found.append((REGISTER_KINDS[letter], int(space or 0), int(number), name))
    return found


def translate(xshader, request, work):
    hlsl = work / f"{request.stem}.hlsl"
    profile = work / f"{request.stem}.profile"
    done = subprocess.run(
        [str(xshader), str(request), "--hlsl-out", str(hlsl), "--profile-out", str(profile)],
        capture_output=True, timeout=300,
    )
    if done.returncode or not hlsl.is_file():
        return None, (done.stderr.decode(errors="replace").strip().splitlines() or ["translation failed"])[-1]
    return hlsl.read_text(encoding="utf-8", errors="replace"), None


def inspect(xshader, request, work):
    """Return (digest, uncovered, refusal) for one request.

    Each uncovered binding is tagged `shadowed` when the same slot holds a
    constant buffer in the signature: the translator asks for an SRV view where
    the title bound a CBV, which a mirrored range could reach.
    """
    digest = request.stem
    payload = rts0_payload(request.read_bytes())
    if payload is None:
        return digest, None, "no root signature"
    try:
        _, flags, params = parse_root(payload)
    except (ValueError, struct.error) as error:
        return digest, None, f"root signature: {error}"
    hlsl, refusal = translate(xshader, request, work)
    if hlsl is None:
        return digest, None, refusal
    uncovered = []
    for kind, space, register, _ in declarations(hlsl):
        if covers(params, flags, kind, space, register) is not None:
            continue
        shadow = (kind == "srv" and covers(params, flags, "cbv", space, register) is not None)
        uncovered.append(f"{kind} {space}:{register}{' (shadowed)' if shadow else ''}")
    return digest, uncovered, None


def attribute(failures, capture, xshader, work):
    """Report, per failing stage, the bindings its own captured signature cannot reach."""
    stages = {}
    for line in failures.read_text(encoding="utf-8", errors="replace").splitlines():
        fields = line.split("\t")
        if len(fields) != 4 or not fields[0].isdigit() or int(fields[0]) <= 0:
            continue
        count, detail = int(fields[0]), fields[3]
        for name, digest in re.findall(r"\b(VS|PS|DS|HS|GS|CS|AS|MS)=([0-9a-f]{64})", detail):
            stages[(name, digest)] = stages.get((name, digest), 0) + count

    reasons = Counter()
    rows = 0
    for (stage, digest), count in sorted(stages.items()):
        shader = capture / "shaders" / f"{digest}.dxbc"
        signatures = sorted((capture / "roots").glob(f"{digest}-*.rootsig"))
        if not signatures or not shader.is_file():
            reasons["stage not captured"] += 1
            continue
        hlsl, refusal = translate(xshader, shader, work)
        if hlsl is None:
            reasons[f"could not translate: {refusal[:60]}"] += 1
            continue
        declared = declarations(hlsl)
        for signature in signatures:
            try:
                _, flags, params = parse_root(signature.read_bytes())
            except (ValueError, struct.error) as error:
                reasons[f"unreadable signature: {error}"] += 1
                continue
            uncovered = [
                (kind, space, register, explain(params, flags, kind, space, register))
                for kind, space, register, _ in declared
                if covers(params, flags, kind, space, register) is None
            ]
            for _, _, _, why in uncovered:
                reasons[why] += 1
            if uncovered:
                rows += 1
                print(f"{stage} {digest[:12]} x{count} root {signature.name.split('-', 1)[1][:12]}")
                for kind, space, register, why in uncovered:
                    print(f"    {kind:8} {space}:{register:<5} {why}")
    print(f"\n{rows} failing stage/signature pairs with an unreachable binding; "
          f"{len(stages)} distinct failing stages in the report")
    for why, count in reasons.most_common(10):
        print(f"  {count:5d}  {why}")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("requests", type=Path, nargs="?", help="directory of recorded request .dxbc files")
    parser.add_argument("--failures", type=Path,
                        help="a pipeline-failures .tsv to attribute instead of scanning requests")
    parser.add_argument("--capture", type=Path,
                        help="the capture directory TUXBOX_CAPTURE_RASTER_ROOTS wrote, for --failures")
    parser.add_argument("--xshader", type=Path, default=DEFAULT_XSHADER)
    parser.add_argument("--work", type=Path, help="scratch directory for emitted HLSL")
    parser.add_argument("--limit", type=int, default=0, help="stop after this many requests")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--show", action="store_true", help="print a line per request with a gap")
    args = parser.parse_args(argv)

    if args.failures is not None:
        if args.capture is None:
            parser.error("--failures needs --capture, the directory the provider wrote")
        from tempfile import TemporaryDirectory
        with TemporaryDirectory(prefix="root-coverage-") as scratch:
            work = args.work or Path(scratch)
            work.mkdir(parents=True, exist_ok=True)
            return attribute(args.failures, args.capture, args.xshader, work)

    if args.requests is None:
        parser.error("give a request directory, or --failures with --capture")

    requests = sorted(args.requests.glob("*.dxbc"))
    if args.limit:
        requests = requests[:args.limit]
    if not requests:
        parser.error(f"no .dxbc requests under {args.requests}")
    if not args.xshader.is_file():
        parser.error(f"translator not found: {args.xshader}")

    from tempfile import TemporaryDirectory
    with TemporaryDirectory(prefix="root-coverage-") as scratch:
        work = args.work or Path(scratch)
        work.mkdir(parents=True, exist_ok=True)
        with ThreadPoolExecutor(args.jobs) as pool:
            results = list(pool.map(lambda r: inspect(args.xshader, r, work), requests))

    gaps, refused, unsigned = [], Counter(), Counter()
    for digest, uncovered, refusal in results:
        if refusal == "no root signature":
            unsigned["request"] += 1
        elif refusal:
            refused[refusal] += 1
        elif uncovered:
            gaps.append((digest, uncovered))
    checked = len(results) - unsigned["request"] - sum(refused.values())
    print(f"{len(results)} requests read; {checked} translated with a root signature; "
          f"{unsigned['request']} carry none; {sum(refused.values())} could not be translated")
    print(f"{len(gaps)} of {checked} translated requests declare a used binding their "
          f"signature cannot reach ({100 * len(gaps) / checked:.1f}%)" if checked else "nothing checked")
    missing = Counter(binding for _, uncovered in gaps for binding in uncovered)
    shadowed = sum(count for binding, count in missing.items() if "(shadowed)" in binding)
    for binding, count in missing.most_common(12):
        print(f"  {count:5d}  {binding}")
    print(f"  {shadowed} of {sum(missing.values())} uncovered bindings sit where the signature "
          f"binds a constant buffer")
    for reason, count in refused.most_common(5):
        print(f"  {count:5d}  refused: {reason[:120]}")
    if args.show:
        for digest, uncovered in gaps[:40]:
            print(f"  {digest} {', '.join(uncovered)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
