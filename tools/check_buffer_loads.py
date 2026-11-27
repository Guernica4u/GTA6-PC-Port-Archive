#!/usr/bin/env python3
"""Read back what translated buffer loads return from buffers of known contents.

Generate the input HLSL with:
  cargo test --manifest-path tools/shader-compiler/Cargo.toml --target x86_64-unknown-linux-gnu buffer_loads_for_the_gpu_check -- --nocapture
Save the text between BEGIN_HLSL and END_HLSL as the --hlsl file. It holds one
program after each `//// name kind first registers` line.

Each program loads a descriptor and reads words through it: scalar loads from a
constant buffer, scalar and vector loads from a raw buffer, and format loads
from a typed buffer. The buffer holds a different known word at every position,
so a load from the wrong place, of the wrong width or in the wrong order shows.
Only the output is instrumented. No game data is needed.
"""
from pathlib import Path
import argparse
import sys, subprocess, struct, json, hashlib
from tempfile import TemporaryDirectory

p = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(p))
from build import find_dxc
from frosty_gpu_guest import run_gpu_guest

WORDS = 384
FIRST_WORD = 96  # the output view starts 384 bytes into the buffer
SOURCE_WORDS = 1024


def pattern(index):
    """The word at `index` of the source buffer: a finite float with the index in its low bits."""
    return 0x41000000 + 0x101 * index


parser = argparse.ArgumentParser(description=__doc__)
for name in ("hlsl", "provider", "proton", "prefix", "compiler-dir", "work", "output"):
    parser.add_argument("--" + name, type=Path, required=True)
parser.add_argument("--adapter", required=True)
args = parser.parse_args()
w = args.work.resolve()
w.mkdir(parents=True, exist_ok=True)
dxc = Path(find_dxc(None)).resolve()


def replace(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new)


programs = {}
for section in args.hlsl.read_text().split("//// ")[1:]:
    header, source = section.split("\n", 1)
    name, kind, first, *registers = header.split()
    source = replace(source, "[numthreads(32, 1, 1)]", "RWByteAddressBuffer output : register(u0);\n[numthreads(32, 1, 1)]")
    stores = "".join(f"    output.Store({4 * j}u, {register});\n" for j, register in enumerate(registers))
    body, closing = source.rstrip().rsplit("}", 1)
    assert not closing.strip(), name
    programs[name] = (kind, int(first), len(registers), body + stores + "}\n")
assert programs, "no programs in the --hlsl file"

RANGES = {"constant": "D3D12_DESCRIPTOR_RANGE_TYPE_CBV", "raw": "D3D12_DESCRIPTOR_RANGE_TYPE_SRV",
          "typed": "D3D12_DESCRIPTOR_RANGE_TYPE_SRV"}
guest = (p / "tools/reference/independent-shaders/compute.cpp").read_text()
guest = replace(guest, "D3D12_DESCRIPTOR_RANGE range{D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 0, 0, 0};", """std::string source_kind;
    if (!(std::ifstream(name + ".kind") >> source_kind))
        throw std::runtime_error("missing buffer kind for " + name);
    D3D12_DESCRIPTOR_RANGE range[]{{D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 0, 0, 0},
        {source_kind == "constant" ? D3D12_DESCRIPTOR_RANGE_TYPE_CBV : D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 1, 0, 1}};""")
guest = replace(guest, "table.DescriptorTable = {1, &range};", "table.DescriptorTable = {2, range};")
guest = replace(guest, "D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 1,", "D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 2,")
guest = replace(guest, "    D3D12_COMMAND_QUEUE_DESC qd{};", f"""    // The source buffer, with a different word at every position.
    const UINT source_words = {SOURCE_WORDS};
    auto source = buffer(device, source_words * 4, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
    {{
        UINT *mapped{{}};
        check(source->Map(0, nullptr, reinterpret_cast<void **>(&mapped)), "map source");
        for (UINT i = 0; i < source_words; ++i)
            mapped[i] = 0x41000000u + 0x101u * i;
        source->Unmap(0, nullptr);
    }}
    auto source_handle = heap->GetCPUDescriptorHandleForHeapStart();
    source_handle.ptr += device->GetDescriptorHandleIncrementSize(hd.Type);
    if (source_kind == "constant") {{
        D3D12_CONSTANT_BUFFER_VIEW_DESC cbv{{source->GetGPUVirtualAddress(), source_words * 4}};
        device->CreateConstantBufferView(&cbv, source_handle);
    }} else {{
        D3D12_SHADER_RESOURCE_VIEW_DESC srv{{}};
        srv.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
        srv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        if (source_kind == "raw") {{
            srv.Format = DXGI_FORMAT_R32_TYPELESS;
            srv.Buffer.NumElements = source_words;
            srv.Buffer.Flags = D3D12_BUFFER_SRV_FLAG_RAW;
        }} else if (source_kind == "typed") {{
            srv.Format = DXGI_FORMAT_R32G32B32A32_FLOAT;
            srv.Buffer.NumElements = source_words / 4;
        }} else {{
            throw std::runtime_error("invalid buffer kind " + source_kind);
        }}
        device->CreateShaderResourceView(source.Get(), &srv, source_handle);
    }}
    D3D12_COMMAND_QUEUE_DESC qd{{}};""")


def digest(data):
    return hashlib.sha256(data).hexdigest()


proton = args.proton.resolve()
with TemporaryDirectory(dir=w) as td:
    trial = Path(td)
    (trial / "guest.cpp").write_text(guest)
    (trial / "context.h").write_text((p / "tools/reference/independent-shaders/context.h").read_text()
                                     .replace('"../compute_buffers.h"', '"' + str(p / "tools/reference/compute_buffers.h") + '"'))
    expected, shaders = {}, {}
    for name, (kind, first, count, source) in programs.items():
        (trial / f"{name}.hlsl").write_text(source)
        blob = trial / f"{name}.cso"
        subprocess.run([str(dxc), "-T", "cs_6_0", "-E", "main", "-Fo", str(blob), str(trial / f"{name}.hlsl")], check=True, timeout=30)
        subprocess.run([str(dxc.with_name("dxv")), str(blob)], check=True, timeout=30)
        shaders[name] = digest(blob.read_bytes())
        (trial / f"{name}.kind").write_text(kind + "\n")
        want = [0x76540000 + i for i in range(WORDS)]
        want[FIRST_WORD:FIRST_WORD + count] = [pattern(first + j) for j in range(count)]
        expected[name] = want
    (trial / "cases.txt").write_text("".join(f"{name} 0 raw 1\n" for name in expected))
    exe, status, fields = run_gpu_guest(trial / "guest.cpp", "buffer-loads.exe", trial, w, proton,
                                        args.prefix.resolve(), args.provider.resolve(), args.compiler_dir.resolve())
    print("guest", status, fields)
    assert status == 0 and fields.get("adapter") == args.adapter
    rows, wrong = [], []
    for name, want in expected.items():
        actual = (trial / f"{name}.bin").read_bytes()
        got = struct.unpack(f"<{WORDS}I", actual)
        differ = [(i - FIRST_WORD, f"{a:#x}", f"{b:#x}") for i, (a, b) in enumerate(zip(got, want)) if a != b]
        if differ:
            wrong.append((name, differ[:6]))
        rows.append({"case": name, "kind": programs[name][0], "words": programs[name][2], "matched": not differ,
                     "sha256": digest(actual)})
    for name, differ in wrong:
        print(f"WRONG {name}: (word, got, wanted) {differ}")
    assert not wrong, f"{len(wrong)} of {len(rows)} cases read the wrong words"
    receipt = {"scope": "Synthetic translated scalar, vector and format loads from constant, raw and typed "
                        "buffers of known contents; instrumented output only",
               "adapter": fields["adapter"], "proton": (proton / "version").read_text().strip(),
               "shader_sha256": shaders, "cases": rows}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"PASS: {len(rows)} cases, {sum(row['words'] for row in rows)} loaded words, 0 mismatches")
