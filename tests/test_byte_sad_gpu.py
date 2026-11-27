import unittest

from tools.check_byte_sad_gpu import reference


class ByteSadReferenceTests(unittest.TestCase):
    # Hand-computed results for the behaviour native RDNA2 execution settled.
    def test_masked_sad_skips_zero_reference_bytes(self):
        # |0x10 - 0x01| + |0x20 - 0x00| skipped + |0x30 - 0x05| + |0x40 - 0x00| skipped
        self.assertEqual(reference("v_msad_u8", 0x4030_2010, 0x0005_0001, [7]), [7 + 0x0F + 0x2B])
        # A zero S0 byte still counts when the S1 byte is set.
        self.assertEqual(reference("v_msad_u8", 0, 0xFF00_0000, [0]), [0xFF])
        self.assertEqual(reference("v_msad_u8", 1, 1, [0xFFFF_FFFF]), [0xFFFF_FFFF])

    def test_quad_windows_slide_by_one_byte(self):
        s0 = 0x0706_0504_0302_0100
        out = reference("v_qsad_pk_u16_u8", s0, 0x0302_0100, [0, 0])
        # Window k is bytes k..k+3, so each byte differs by k.
        self.assertEqual(out, [0 | 4 << 16, 8 | 12 << 16])

    def test_sixteen_bit_fields_wrap(self):
        out = reference("v_qsad_pk_u16_u8", 0xFF, 0, [0xFFFF_FFF0, 0])
        self.assertEqual(out[0], 0xFFFF_00EF)  # 0xfff0 + 0xff wraps to 0xef
        self.assertEqual(reference("v_mqsad_pk_u16_u8", 0xFF, 0, [0xFFF0, 0]), [0xFFF0, 0])

    def test_mqsad_u32_uses_one_accumulator_word_per_window(self):
        out = reference("v_mqsad_u32_u8", 0x0000_0000_0000_00FF, 0x0000_0001, [1, 2, 3, 4])
        self.assertEqual(out, [1 + 0xFE, 2 + 1, 3 + 1, 4 + 1])


if __name__ == "__main__":
    unittest.main()
