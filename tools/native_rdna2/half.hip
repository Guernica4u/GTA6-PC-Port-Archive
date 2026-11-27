// Half-precision and normalized-pack instructions, one per lane, for
// tools/native_rdna2/check_half.py. Same record layout and host interface as
// kernels.hip: S0 is f, S1 is c, and the destination starts as the low word
// of b, so the result shows which destination bits an instruction keeps.
#include <hip/hip_runtime.h>
#include <cstdint>

struct In { uint64_t a, b; uint32_t c, f; };

#define SET "s_setreg_b32 hwreg(HW_REG_MODE, 0, 10), %[m]\n s_nop 7\n"
#define K(op, body) extern "C" __global__ void k_##op(const In* in, uint64_t* out, uint32_t mode) { \
    int i = blockIdx.x * 64 + threadIdx.x; In x = in[i]; uint32_t r = (uint32_t)x.b; body; out[i] = r; }
#define UNARY(op) K(op, { asm volatile(SET #op " %0, %1" : "+v"(r) : "v"(x.f), [m]"s"(mode)); })
#define BINARY(op) K(op, { asm volatile(SET #op " %0, %1, %2" : "+v"(r) : "v"(x.f), "v"(x.c), [m]"s"(mode)); })

UNARY(v_frexp_mant_f16)
UNARY(v_frexp_exp_i16_f16)
BINARY(v_ldexp_f16)
BINARY(v_cvt_pknorm_i16_f16)
BINARY(v_cvt_pknorm_u16_f16)
BINARY(v_cvt_pknorm_i16_f32)
BINARY(v_cvt_pknorm_u16_f32)
