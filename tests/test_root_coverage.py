import struct
import unittest

from tools.report_root_coverage import covers, declarations, parse_root, rts0_payload


def serialized(flags=0, table=(), constants=None):
    """A version 1.1 RTS0 payload with one table and optional root constants.

    `table` holds (range kind word, count, base register, space) rows.
    """
    header = 24
    params = 2 if constants else 1
    param_offset = header
    body_offset = param_offset + 12 * params

    table_bytes = b""
    if table:
        ranges_offset = body_offset + 8
        stride = 24
        rows = b"".join(
            struct.pack("<6I", kind, count, base, space, 0, 0xFFFFFFFF)
            for kind, count, base, space in table
        )
        table_bytes = struct.pack("<2I", len(table), ranges_offset) + rows
    constants_offset = body_offset + len(table_bytes)
    constants_bytes = struct.pack("<3I", *constants) if constants else b""

    payload = struct.pack("<6I", 2, params, param_offset, 0, 0, flags)
    payload += struct.pack("<3I", 0 if table else 1, 0, body_offset)
    if constants:
        payload += struct.pack("<3I", 1, 0, constants_offset)
    return payload + table_bytes + constants_bytes


def container(payload):
    """A DXBC container holding `payload` as its only chunk."""
    chunk = b"RTS0" + struct.pack("<I", len(payload)) + payload
    header = bytearray(36)
    header[:4] = b"DXBC"
    struct.pack_into("<I", header, 28, 1)
    struct.pack_into("<I", header, 32, 36)  # first chunk starts after the chunk table
    return bytes(header) + chunk


class RootCoverage(unittest.TestCase):
    def test_a_table_range_covers_only_its_own_space_and_kind(self):
        # The shape the retail root signatures use: a CBV at space 1 register 16
        # while the translated program asks for an SRV there.
        version, flags, params = parse_root(
            serialized(table=[(0, 1, 16, 1), (1, 1, 7, 0)])
        )
        self.assertEqual(version, 2)
        self.assertEqual(flags, 0)
        self.assertEqual(covers(params, flags, "srv", 0, 7), "table")
        self.assertIsNone(covers(params, flags, "srv", 1, 16))
        self.assertIsNone(covers(params, flags, "srv", 0, 8))
        self.assertIsNone(covers(params, flags, "cbv", 0, 7))

    def test_a_range_reaches_every_register_it_declares(self):
        _, flags, params = parse_root(serialized(table=[(1, 8, 232, 1)]))
        self.assertEqual(covers(params, flags, "srv", 1, 232), "table")
        self.assertEqual(covers(params, flags, "srv", 1, 239), "table")
        self.assertIsNone(covers(params, flags, "srv", 1, 240))

    def test_root_constants_cover_the_constant_buffer_at_their_slot(self):
        _, flags, params = parse_root(serialized(constants=(500, 0, 4)))
        self.assertEqual(covers(params, flags, "cbv", 0, 500), "constants")
        self.assertIsNone(covers(params, flags, "srv", 0, 500))

    def test_a_directly_indexed_heap_covers_any_srv_but_not_a_sampler(self):
        _, flags, params = parse_root(serialized(flags=0x400, table=[(1, 1, 0, 0)]))
        self.assertEqual(covers(params, flags, "srv", 1, 239), "heap")
        self.assertIsNone(covers(params, flags, "sampler", 1, 239))

    def test_a_declaration_is_reported_only_when_the_program_reads_it(self):
        hlsl = (
            "StructuredBuffer<TbElem_sbuf16> sbuf16 : register(t16, space1);\n"
            "uint tb_sbw_sbuf16(uint off) { uint n, st; sbuf16.GetDimensions(n, st); return 0u; }\n"
            "Texture2D<float4> tex24 : register(t24);\n"
            "cbuffer CB5 : register(b5, space1) { uint4 cb5[1]; };\n"
            "RWTexture2D<float4> out0 : register(u3);\n"
            "out0[uint2(0, 0)] = tex24.Sample(samp, float2(0, 0));\n"
        )
        found = {(kind, space, register) for kind, space, register, _ in declarations(hlsl)}
        self.assertIn(("srv", 1, 16), found)
        self.assertIn(("srv", 0, 24), found)
        self.assertIn(("uav", 0, 3), found)
        self.assertNotIn(("cbv", 1, 5), found, "a declaration nobody reads is not a binding")

    def test_the_root_signature_is_read_from_its_rts0_chunk(self):
        payload = serialized(table=[(1, 1, 7, 0)])
        self.assertEqual(rts0_payload(container(payload)), payload)
        self.assertIsNone(rts0_payload(b"DXBC" + bytes(28)))


if __name__ == "__main__":
    unittest.main()
