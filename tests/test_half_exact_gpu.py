import unittest

from tools.check_half_exact_gpu import reference

DEFAULT, IEEE, FLUSH = 0xC0, 0x2C0, 0x000
OLD = 0xABCD_0000


def run(name, s0, s1=0, mode=DEFAULT):
    return reference(name, s0, s1, OLD | 0x1234, mode)


class HalfExactReferenceTests(unittest.TestCase):
    # Hand-computed results for the behaviour native RDNA2 execution settled.
    def test_frexp_keeps_the_high_half(self):
        self.assertEqual(run("v_frexp_mant_f16", 0x4200), OLD | 0x3A00)  # 3.0 -> 0.75
        self.assertEqual(run("v_frexp_exp_i16_f16", 0x4200), OLD | 2)
        self.assertEqual(run("v_frexp_exp_i16_f16", 0x0001), OLD | (-23 & 0xFFFF))  # 2^-24 = 0.5 * 2^-23
        self.assertEqual(run("v_frexp_exp_i16_f16", 0x0001, mode=FLUSH), OLD)
        self.assertEqual(run("v_frexp_mant_f16", 0x8001, mode=FLUSH), OLD | 0x8000)
        self.assertEqual(run("v_frexp_mant_f16", 0x7C01), OLD | 0x7C01)
        self.assertEqual(run("v_frexp_mant_f16", 0x7C01, mode=IEEE), OLD | 0x7E01)

    def test_ldexp_uses_a_signed_16_bit_exponent(self):
        self.assertEqual(run("v_ldexp_f16", 0x3C00, 3), OLD | 0x4800)  # 1.0 * 2^3
        self.assertEqual(run("v_ldexp_f16", 0x3C00, 0xFFFF_0003), OLD | 0x4800)
        self.assertEqual(run("v_ldexp_f16", 0x3C00, 0x8000), OLD)  # 2^-32768 underflows
        self.assertEqual(run("v_ldexp_f16", 0x3C00, 16), OLD | 0x7C00)
        self.assertEqual(run("v_ldexp_f16", 0x3C00, 16, mode=DEFAULT | 0xC), OLD | 0x7BFF)  # toward zero

    def test_ldexp_rounds_denormal_results_by_mode(self):
        # 1.5 * 2^-24 lies halfway between the two smallest denormals.
        self.assertEqual(run("v_ldexp_f16", 0x3E00, -24 & 0xFFFF), OLD | 0x0002)  # ties to even
        self.assertEqual(run("v_ldexp_f16", 0x3E00, -24 & 0xFFFF, mode=DEFAULT | 0xC), OLD | 0x0001)
        self.assertEqual(run("v_ldexp_f16", 0x3E00, -24 & 0xFFFF, mode=0x40), OLD)  # f16 output denormals off

    def test_packs_replace_the_word(self):
        self.assertEqual(run("v_cvt_pknorm_i16_f16", 0x3C00, 0xBC00), 0x8001_7FFF)
        self.assertEqual(run("v_cvt_pknorm_u16_f16", 0x3800, 0xBC00), 0x0000_8000)  # 0.5 * 65535 ties to even
        self.assertEqual(run("v_cvt_pknorm_i16_f32", 0x3F00_0000, 0x7FC0_0000), 0x0000_4000)  # 0.5 * 32767 -> 16384
        self.assertEqual(run("v_cvt_pknorm_i16_f32", 0xFF80_0000, 0x0000_0001), 0x0000_8001)
        self.assertEqual(run("v_cvt_pknorm_u16_f32", 0x3F80_0001, 0xBF00_0000), 0x0000_FFFF)


if __name__ == "__main__":
    unittest.main()
