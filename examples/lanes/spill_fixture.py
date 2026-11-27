"""The synthetic Xbox raster program of the `lane-spill-interp` self-test.

A small hand-written console program, not taken from any game. The pixel program
writes a lane-spill area in v128 with `v_writelane_b32`, then runs an f16 raster
interpolator. The interpolator's SRC0 field is an attribute selector (the RDNA 2
instruction reference types it OPR_ATTR): its attribute, channel and high-half
bits here read 384, which is also how v128 would be encoded as a register. A
translator that takes it for a register refuses the program as a spill area
also used as a vector.

The f16 interpolator words are hand-encoded from the RDNA 2 instruction layout
(llvm-mc rejects this attribute form). The spill and export words are from
llvm-mc.
"""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from examples.lanes.raster_fixture import image, signature, validation, container  # noqa: E402

# gfx1030. The interpolator is VOP3 opcode 834 (v_interp_p1ll_f16) with dst v14.
# Its SRC0 field is 384: attribute 0, channel 2, high half. SRC1 is the barycentric
# in v0.
PIXEL_CODE = (
    0xD7610080, 0x02010000,  # v_writelane_b32 v128, s0, 0   spill area (llvm-mc)
    0xD742000E, 0x00020180,  # v_interp_p1ll_f16 v14, attr0.z (high half), v0
    0x7E0802FF, 0x7AB1E003,  # v_mov_b32 v4, 0x7AB1E003   constant result (llvm-mc)
    0xF8180001, 0x00000004,  # exp mrt0 v4, off, off, off done vm
    0xBF810000,              # s_endpgm
)
# The constant the pixel program writes to the target: 0x7AB1E003.
EXPECTED = 0x7AB1E003


def pixel():
    return container(0x60, image(5, PIXEL_CODE, (), (), inputs=1), [
        (b"ISGN", signature([
            ("SV_Position", 0, 1, 3, 0, 15, 0),
            ("TEXCOORD", 0, 0, 3, 1, 15, 15),
        ])),
        (b"OSGN", signature([("SV_Target", 0, 64, 1, 0, 1, 0)])),
        (b"PSV0", validation(0)),
    ])


def declaration(name, data):
    values = ",".join(str(byte) for byte in data)
    return f"constexpr unsigned char {name}[] = {{{values}}};\n"


def header():
    return ("// Generated from spill_fixture.py.\n"
            + declaration("lane_spill_pixel", pixel())
            + f"constexpr UINT lane_spill_expected = {EXPECTED:#x};\n")


if __name__ == "__main__":
    Path(sys.argv[1]).write_text(header())
