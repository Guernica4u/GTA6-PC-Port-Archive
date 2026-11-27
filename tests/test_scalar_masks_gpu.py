import unittest

from tools.check_scalar_masks_gpu import bitreplicate, combine, expected_outputs, quadmask

M32, M64 = 0xFFFF_FFFF, (1 << 64) - 1


class ScalarMasksReferenceTests(unittest.TestCase):
    # Hand-computed results from the RDNA2 ISA pseudocode.
    def test_quadmask(self):
        self.assertEqual(quadmask(0x1357_0F00, 8), 0b1111_0100)
        self.assertEqual(quadmask(0x8000_0001, 8), 0b1000_0001)
        self.assertEqual(quadmask(0x0000_0001 << 32 | 0xF000_0000, 16), 1 << 8 | 1 << 7)
        self.assertEqual(quadmask(0, 16), 0)

    def test_bitreplicate(self):
        self.assertEqual(bitreplicate(0b101), 0b11_00_11)
        self.assertEqual(bitreplicate(0x8000_0000), 0xC000_0000_0000_0000)
        self.assertEqual(bitreplicate(M32), M64)

    def test_saveexec(self):
        self.assertEqual(combine("s_nor_saveexec", 0b0011, 0b0101, M32), M32 & ~0b0111)
        self.assertEqual(combine("s_xnor_saveexec", 0b0011, 0b0101, M32), M32 & ~0b0110)
        self.assertEqual(combine("s_orn1_saveexec", 0b0011, 0b0101, M32), M32 & ~0b0011 | 0b0101)

    def test_saveexec_rows_keep_the_old_mask_and_lane(self):
        out = expected_outputs(1, 0, 1, 0)
        # Quad masks and replication: 12 words. nor_b32 then saves EXEC = 1,
        # sets SCC, and leaves bit 0 (this lane) clear.
        self.assertEqual(out[12:15], [1, 1, M32 & ~1])
        # xnor and orn1 keep the lane: ~(1 ^ 1) and ~1 | 1 have bit 0 set.
        self.assertEqual(out[-6:], [0, 1, 1, 0, 1, 1])


if __name__ == "__main__":
    unittest.main()
