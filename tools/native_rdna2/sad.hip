// Byte SAD instructions, one per lane, for tools/native_rdna2/check_sad.py.
// Same record layout and host interface as kernels.hip.
#include <hip/hip_runtime.h>
#include <cstdint>

struct In { uint64_t a, b; uint32_t c, f; };

#define SET "s_setreg_b32 hwreg(HW_REG_MODE, 0, 10), %[m]\n s_nop 7\n"
#define K(op, body) extern "C" __global__ void k_##op(const In* in, uint64_t* out, uint32_t mode) { \
    int i = blockIdx.x * 64 + threadIdx.x; In x = in[i]; uint64_t r = 0; uint32_t r32 = 0; (void)r32; body; out[i] = r; }

// Byte SAD forms. MODE does not apply; S0 is a, S1 is c, S2 is b. The
// 128-bit accumulator of v_mqsad_u32_u8 is (b, f, low ^ high word of b),
// and its result is stored in two halves.
typedef uint32_t u32x4 __attribute__((ext_vector_type(4)));
K(v_msad_u8, { asm volatile(SET "v_msad_u8 %0, %1, %2, %3" : "=&v"(r32) : "v"((uint32_t)x.a), "v"(x.c), "v"((uint32_t)x.b), [m]"s"(mode)); r = r32; })
K(v_qsad_pk_u16_u8, { asm volatile(SET "v_qsad_pk_u16_u8 %0, %1, %2, %3" : "=&v"(r) : "v"(x.a), "v"(x.c), "v"(x.b), [m]"s"(mode)); })
K(v_mqsad_pk_u16_u8, { asm volatile(SET "v_mqsad_pk_u16_u8 %0, %1, %2, %3" : "=&v"(r) : "v"(x.a), "v"(x.c), "v"(x.b), [m]"s"(mode)); })
__device__ uint64_t mqsad(const In& x, int half, uint32_t mode) {
    u32x4 acc = {(uint32_t)x.b, (uint32_t)(x.b >> 32), x.f, (uint32_t)x.b ^ (uint32_t)(x.b >> 32)};
    u32x4 d;
    asm volatile(SET "v_mqsad_u32_u8 %0, %1, %2, %3" : "=&v"(d) : "v"(x.a), "v"(x.c), "v"(acc), [m]"s"(mode));
    return (uint64_t)d[2 * half] | (uint64_t)d[2 * half + 1] << 32;
}
K(v_mqsad_u32_u8_0, { r = mqsad(x, 0, mode); })
K(v_mqsad_u32_u8_1, { r = mqsad(x, 1, mode); })
