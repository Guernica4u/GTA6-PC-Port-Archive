import struct
import unittest

from tools.check_f64_modes_gpu import reference

DEFAULT, IEEE, FLUSH = 0xC0, 0x2C0, 0x000


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def f32(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def run(name, a=0, b=0, c=0, f=0, mode=DEFAULT):
    return reference(name, a, b, c, f, mode)


class F64ModesReferenceTests(unittest.TestCase):
    # Hand-computed results for the cases where RDNA2 behaviour is easy to get wrong.
    def test_f32_conversion_flushes_on_tininess_after_rounding(self):
        smallest_normal = 2.0**-126
        # Exactly representable with 24 bits below 2^-126: tiny, flushed.
        self.assertEqual(run("v_cvt_f32_f64", bits(smallest_normal - 2.0**-150), mode=FLUSH), 0)
        # Rounds up to 2^-126 at full precision: not tiny, kept.
        self.assertEqual(run("v_cvt_f32_f64", bits(smallest_normal - 2.0**-151), mode=FLUSH), 0x0080_0000)
        # With f32 output denormals enabled it rounds to nearest even, up to 2^-126.
        self.assertEqual(run("v_cvt_f32_f64", bits(smallest_normal - 2.0**-150), mode=0xF0), 0x0080_0000)
        self.assertEqual(run("v_cvt_f32_f64", bits(smallest_normal - 2.0**-149), mode=0xF0), 0x007F_FFFF)
        self.assertEqual(run("v_cvt_f32_f64", bits(1 / 3)), f32(1 / 3))
        # Round toward zero overflows to the largest finite value.
        self.assertEqual(run("v_cvt_f32_f64", bits(1e300), mode=DEFAULT | 3), 0x7F7F_FFFF)
        self.assertEqual(run("v_cvt_f32_f64", 0x7FF0_0000_0000_0001), 0x7FC0_0000)

    def test_nan_quieting_follows_ieee_mode(self):
        signaling = 0x7FF0_0000_0000_0001
        for name in ("v_floor_f64", "v_rndne_f64", "v_fract_f64", "v_frexp_mant_f64", "v_ldexp_f64"):
            self.assertEqual(run(name, signaling), signaling, name)
            self.assertEqual(run(name, signaling, mode=IEEE), signaling | 1 << 51, name)
        self.assertEqual(run("v_cvt_f64_f32", f=0x7F80_0001), 0x7FF0_0000_2000_0000)
        self.assertEqual(run("v_cvt_f64_f32", f=0x7F80_0001, mode=IEEE), 0x7FF8_0000_2000_0000)

    def test_extrema(self):
        self.assertEqual(run("v_min_f64", bits(-0.0), bits(0.0)), bits(-0.0))
        self.assertEqual(run("v_max_f64", bits(-0.0), bits(0.0)), bits(0.0))
        signaling = 0x7FF0_0000_0000_0001
        self.assertEqual(run("v_min_f64", bits(1.0), signaling), bits(1.0))
        self.assertEqual(run("v_min_f64", bits(1.0), signaling, mode=IEEE), signaling | 1 << 51)
        self.assertEqual(run("v_max_f64", bits(1.0), 0x7FF8_0000_0000_0000, mode=IEEE), bits(1.0))
        # A flushed denormal compares as a signed zero.
        self.assertEqual(run("v_max_f64", 0x8000_0000_0000_0001, bits(-0.0), mode=FLUSH), bits(-0.0))

    def test_frexp(self):
        self.assertEqual(run("v_frexp_mant_f64", bits(3.0)), bits(0.75))
        self.assertEqual(run("v_frexp_exp_i32_f64", bits(3.0)), 2)
        self.assertEqual(run("v_frexp_exp_i32_f64", 1), (-1073) % 2**32)
        self.assertEqual(run("v_frexp_exp_i32_f64", 1, mode=FLUSH), 0)
        self.assertEqual(run("v_frexp_mant_f64", 1), bits(0.5))
        self.assertEqual(run("v_frexp_exp_i32_f32", f=f32(-0.375)), (-1) % 2**32)
        self.assertEqual(run("v_frexp_mant_f32", f=f32(-0.375)), f32(-0.75))
        self.assertEqual(run("v_frexp_mant_f32", f=1), 0)  # f32 input denormals are flushed by default

    def test_fract(self):
        self.assertEqual(run("v_fract_f64", bits(-0.25)), bits(0.75))
        self.assertEqual(run("v_fract_f64", bits(-0.0)), 0)
        self.assertEqual(run("v_fract_f64", bits(-1e-17)), 0x3FEF_FFFF_FFFF_FFFF)
        self.assertEqual(run("v_fract_f64", bits(float("inf"))), 0xFFF8_0000_0000_0000)
        # Toward -inf, 1 - 1e-17 rounds down to the largest double below 1 as well.
        self.assertEqual(run("v_fract_f64", bits(-1e-17), mode=DEFAULT | 8), 0x3FEF_FFFF_FFFF_FFFF)

    def test_ldexp(self):
        self.assertEqual(run("v_ldexp_f64", bits(1.5), c=3), bits(12.0))
        self.assertEqual(run("v_ldexp_f64", bits(1.0), c=1024), bits(float("inf")))
        self.assertEqual(run("v_ldexp_f64", bits(1.0), c=1024, mode=DEFAULT | 0xC), 0x7FEF_FFFF_FFFF_FFFF)
        # (2^53 - 1) * 2^-1075 rounds to 2^-1022, but is tiny before and after rounding.
        self.assertEqual(run("v_ldexp_f64", 0x432F_FFFF_FFFF_FFFF, c=(-1074) % 2**32), 0x0010_0000_0000_0000)
        self.assertEqual(run("v_ldexp_f64", 0x432F_FFFF_FFFF_FFFF, c=(-1074) % 2**32, mode=0x40), 0)
        # The exponent is a signed 32-bit integer.
        self.assertEqual(run("v_ldexp_f64", bits(1.0), c=2**31 - 1), bits(float("inf")))
        self.assertEqual(run("v_ldexp_f64", bits(1.0), c=2**31), 0)


if __name__ == "__main__":
    unittest.main()
