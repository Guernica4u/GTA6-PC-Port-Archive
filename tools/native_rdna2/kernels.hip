// One RDNA2 instruction per lane under an explicit MODE[9:0] value, for
// comparing reference models with native execution. Built with
// hipcc --genco; host.c loads the code object through the HSA runtime.
#include <hip/hip_runtime.h>
#include <cstdint>

struct In { uint64_t a, b; uint32_t c, f; };

// MODE takes effect for the following VALU instructions after a few wait states.
#define SET "s_setreg_b32 hwreg(HW_REG_MODE, 0, 10), %[m]\n s_nop 7\n"
#define K(op, body) extern "C" __global__ void k_##op(const In* in, uint64_t* out, uint32_t mode) { \
    int i = blockIdx.x * 64 + threadIdx.x; In x = in[i]; uint64_t r = 0; uint32_t r32 = 0; (void)r32; body; out[i] = r; }
#define PAIR1(op) K(op, { asm volatile(SET #op " %0, %1" : "=&v"(r) : "v"(x.a), [m]"s"(mode)); })
#define PAIR_TO_WORD(op) K(op, { asm volatile(SET #op " %0, %1" : "=&v"(r32) : "v"(x.a), [m]"s"(mode)); r = r32; })
#define WORD1(op) K(op, { asm volatile(SET #op " %0, %1" : "=&v"(r32) : "v"(x.f), [m]"s"(mode)); r = r32; })

K(v_cvt_f64_f32, { asm volatile(SET "v_cvt_f64_f32 %0, %1" : "=&v"(r) : "v"(x.f), [m]"s"(mode)); })
PAIR_TO_WORD(v_cvt_f32_f64)
K(v_min_f64, { asm volatile(SET "v_min_f64 %0, %1, %2" : "=&v"(r) : "v"(x.a), "v"(x.b), [m]"s"(mode)); })
K(v_max_f64, { asm volatile(SET "v_max_f64 %0, %1, %2" : "=&v"(r) : "v"(x.a), "v"(x.b), [m]"s"(mode)); })
PAIR_TO_WORD(v_frexp_exp_i32_f64)
PAIR1(v_frexp_mant_f64)
PAIR1(v_fract_f64)
K(v_ldexp_f64, { asm volatile(SET "v_ldexp_f64 %0, %1, %2" : "=&v"(r) : "v"(x.a), "v"(x.c), [m]"s"(mode)); })
WORD1(v_frexp_exp_i32_f32)
WORD1(v_frexp_mant_f32)
PAIR1(v_trunc_f64)
PAIR1(v_floor_f64)
PAIR1(v_ceil_f64)
PAIR1(v_rndne_f64)
