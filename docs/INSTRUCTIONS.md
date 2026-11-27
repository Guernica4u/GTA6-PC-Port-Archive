# RDNA 2 instructions

Every instruction the decoder names, from [AMD's machine-readable instruction set](https://gpuopen.com/machine-readable-isa/) for RDNA 1 and RDNA 2, and how far TuxBox supports it. This file is generated: see [how to update it](SHADER_HARNESS.md#the-instruction-inventory). Don't edit it by hand.

| Column | Meaning |
| --- | --- |
| Translator | The result of translating the instruction alone, with plain register operands. **Translates** means some statement was emitted, not that the result is correct |
| GPU check | Cards on which a [translator GPU check](../tools/check_translator_gpu.py) ran a program holding the instruction. It doesn't show that every operand form or lane state was tested |
| Recorded shaders | Translated shaders of ProjectScarlett 1.0.2.1 that hold it, of 44,799 |

## Summary

- 1,085 instructions in 1,398 encodings.
- 372 are used by the recorded shaders; a GPU check runs 369 of those.
- 210 encodings have no translation, and 186 are unproven: alone they need a bound resource or a stage, and no recorded shader uses them.

| Family | Encodings | Translates | Seen translating | Nothing to emit | Unproven | Not translated | Used | Used without a GPU check |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| [Scalar, two operands (SOP2)](#scalar-two-operands-sop2) | 51 | 51 | 0 | 0 | 0 | 0 | 29 | 0 |
| [Scalar with a constant (SOPK)](#scalar-with-a-constant-sopk) | 27 | 19 | 0 | 5 | 0 | 3 | 11 | 0 |
| [Scalar, one operand (SOP1)](#scalar-one-operand-sop1) | 65 | 44 | 0 | 2 | 13 | 6 | 24 | 2 |
| [Scalar compare (SOPC)](#scalar-compare-sopc) | 18 | 16 | 0 | 0 | 2 | 0 | 12 | 0 |
| [Program control (SOPP)](#program-control-sopp) | 37 | 11 | 0 | 6 | 1 | 19 | 12 | 0 |
| [Scalar memory (SMEM)](#scalar-memory-smem) | 20 | 0 | 5 | 5 | 0 | 10 | 9 | 0 |
| [Vector, two operands (VOP2)](#vector-two-operands-vop2) | 50 | 49 | 0 | 0 | 0 | 1 | 38 | 0 |
| [Vector, one operand (VOP1)](#vector-one-operand-vop1) | 81 | 72 | 0 | 1 | 0 | 8 | 41 | 0 |
| [Vector compare (VOPC)](#vector-compare-vopc) | 190 | 190 | 0 | 0 | 0 | 0 | 61 | 0 |
| [Vector, three operands (VOP3 and VOP3P)](#vector-three-operands-vop3-and-vop3p) | 457 | 421 | 7 | 1 | 1 | 27 | 199 | 0 |
| [Interpolation (VINTRP)](#interpolation-vintrp) | 3 | 0 | 3 | 0 | 0 | 0 | 3 | 0 |
| [Group memory (DS)](#group-memory-ds) | 123 | 36 | 0 | 0 | 0 | 87 | 17 | 0 |
| [Flat, global and scratch memory (FLAT)](#flat-global-and-scratch-memory-flat) | 54 | 0 | 5 | 0 | 0 | 49 | 5 | 1 |
| [Untyped buffers (MUBUF)](#untyped-buffers-mubuf) | 75 | 2 | 25 | 0 | 48 | 0 | 25 | 0 |
| [Typed buffers (MTBUF)](#typed-buffers-mtbuf) | 16 | 0 | 4 | 0 | 12 | 0 | 4 | 0 |
| [Images (MIMG)](#images-mimg) | 130 | 0 | 21 | 0 | 109 | 0 | 21 | 0 |
| [Export (EXP)](#export-exp) | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 |

## Used without a GPU check

Most used first. Each needs a check before it can be trusted.

| Instruction | Recorded shaders |
| --- | ---: |
| `s_swappc_b64` | 679 |
| `s_setpc_b64` | 6 |
| `flat_load_ushort` | 4 |

## Scalar, two operands (SOP2)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `s_add_u32` | 0x0 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 10,084 |
| `s_sub_u32` | 0x1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_add_i32` | 0x2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 37,128 |
| `s_sub_i32` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12,875 |
| `s_addc_u32` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,787 |
| `s_subb_u32` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_min_i32` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 92 |
| `s_min_u32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,639 |
| `s_max_i32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 129 |
| `s_max_u32` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 166 |
| `s_cselect_b32` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 25,418 |
| `s_cselect_b64` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 24,269 |
| `s_and_b32` | 0xe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 36,240 |
| `s_and_b64` | 0xf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 26,683 |
| `s_or_b32` | 0x10 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 30,412 |
| `s_or_b64` | 0x11 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,068 |
| `s_xor_b32` | 0x12 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 10,635 |
| `s_xor_b64` | 0x13 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,139 |
| `s_andn2_b32` | 0x14 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,617 |
| `s_andn2_b64` | 0x15 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 23,336 |
| `s_orn2_b32` | 0x16 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8 |
| `s_orn2_b64` | 0x17 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_nand_b32` | 0x18 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_nand_b64` | 0x19 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_nor_b32` | 0x1a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_nor_b64` | 0x1b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_xnor_b32` | 0x1c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_xnor_b64` | 0x1d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_lshl_b32` | 0x1e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 37,628 |
| `s_lshl_b64` | 0x1f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 712 |
| `s_lshr_b32` | 0x20 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,325 |
| `s_lshr_b64` | 0x21 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14 |
| `s_ashr_i32` | 0x22 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 944 |
| `s_ashr_i64` | 0x23 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_bfm_b32` | 0x24 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 833 |
| `s_bfm_b64` | 0x25 | Yes | — | — |
| `s_mul_i32` | 0x26 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 29,832 |
| `s_bfe_u32` | 0x27 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 27,456 |
| `s_bfe_i32` | 0x28 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,811 |
| `s_bfe_u64` | 0x29 | Yes | — | — |
| `s_bfe_i64` | 0x2a | Yes | — | — |
| `s_absdiff_i32` | 0x2c | Yes | — | — |
| `s_lshl1_add_u32` | 0x2e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_lshl2_add_u32` | 0x2f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_lshl3_add_u32` | 0x30 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_lshl4_add_u32` | 0x31 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_pack_ll_b32_b16` | 0x32 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_pack_lh_b32_b16` | 0x33 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_pack_hh_b32_b16` | 0x34 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_mul_hi_u32` | 0x35 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12,480 |
| `s_mul_hi_i32` | 0x36 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |

## Scalar with a constant (SOPK)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `s_movk_i32` | 0x0 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 20,137 |
| `s_version` | 0x1 | Nothing to emit | — | — |
| `s_cmovk_i32` | 0x2 | Yes | — | — |
| `s_cmpk_eq_i32` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 3,397 |
| `s_cmpk_lg_i32` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,408 |
| `s_cmpk_gt_i32` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 354 |
| `s_cmpk_ge_i32` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_cmpk_lt_i32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_cmpk_le_i32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_cmpk_eq_u32` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,007 |
| `s_cmpk_lg_u32` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_cmpk_gt_u32` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 167 |
| `s_cmpk_ge_u32` | 0xc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_cmpk_lt_u32` | 0xd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8 |
| `s_cmpk_le_u32` | 0xe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_addk_i32` | 0xf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 15,890 |
| `s_mulk_i32` | 0x10 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 7,107 |
| `s_getreg_b32` | 0x12 | Yes | — | — |
| `s_setreg_b32` | 0x13 | Yes | — | — |
| `s_setreg_imm32_b32` | 0x15 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 28,099 |
| `s_call_b64` | 0x16 | **No** | — | — |
| `s_waitcnt_vscnt` | 0x17 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 180 |
| `s_waitcnt_vmcnt` | 0x18 | Nothing to emit | — | — |
| `s_waitcnt_expcnt` | 0x19 | Nothing to emit | — | — |
| `s_waitcnt_lgkmcnt` | 0x1a | Nothing to emit | — | — |
| `s_subvector_loop_begin` | 0x1b | **No** | — | — |
| `s_subvector_loop_end` | 0x1c | **No** | — | — |

## Scalar, one operand (SOP1)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `s_mov_b32` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 44,140 |
| `s_mov_b64` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 31,780 |
| `s_cmov_b32` | 0x5 | Yes | — | — |
| `s_cmov_b64` | 0x6 | Yes | — | — |
| `s_not_b32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,663 |
| `s_not_b64` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 98 |
| `s_wqm_b32` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,089 |
| `s_wqm_b64` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,892 |
| `s_brev_b32` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 15,586 |
| `s_brev_b64` | 0xc | Unproven: s_brev_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_bcnt0_i32_b32` | 0xd | Unproven: s_bcnt0_i32_b32 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_bcnt0_i32_b64` | 0xe | Unproven: s_bcnt0_i32_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_bcnt1_i32_b32` | 0xf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 353 |
| `s_bcnt1_i32_b64` | 0x10 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 886 |
| `s_ff0_i32_b32` | 0x11 | Unproven: s_ff0_i32_b32 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_ff0_i32_b64` | 0x12 | Unproven: s_ff0_i32_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_ff1_i32_b32` | 0x13 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,763 |
| `s_ff1_i32_b64` | 0x14 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 570 |
| `s_flbit_i32_b32` | 0x15 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 376 |
| `s_flbit_i32_b64` | 0x16 | Unproven: s_flbit_i32_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_flbit_i32` | 0x17 | Unproven: s_flbit_i32 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_flbit_i32_i64` | 0x18 | Unproven: s_flbit_i32_i64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_sext_i32_i8` | 0x19 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_sext_i32_i16` | 0x1a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_bitset0_b32` | 0x1b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,034 |
| `s_bitset0_b64` | 0x1c | Unproven: s_bitset0_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_bitset1_b32` | 0x1d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,591 |
| `s_bitset1_b64` | 0x1e | Unproven: s_bitset1_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_getpc_b64` | 0x1f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,787 |
| `s_setpc_b64` | 0x20 | Nothing to emit | — | 6 |
| `s_swappc_b64` | 0x21 | Nothing to emit | — | 679 |
| `s_rfe_b64` | 0x22 | **No** | — | — |
| `s_and_saveexec_b64` | 0x24 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,637 |
| `s_or_saveexec_b64` | 0x25 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 823 |
| `s_xor_saveexec_b64` | 0x26 | Yes | — | — |
| `s_andn2_saveexec_b64` | 0x27 | Yes | — | — |
| `s_orn2_saveexec_b64` | 0x28 | Yes | — | — |
| `s_nand_saveexec_b64` | 0x29 | Yes | — | — |
| `s_nor_saveexec_b64` | 0x2a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_xnor_saveexec_b64` | 0x2b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_quadmask_b32` | 0x2c | Unproven: s_quadmask_b32 needs proven ordinary scalar operands (not a wave mask) | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_quadmask_b64` | 0x2d | Unproven: s_quadmask_b64 needs proven ordinary scalar operands (not a wave mask) | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_movrels_b32` | 0x2e | **No** | — | — |
| `s_movrels_b64` | 0x2f | **No** | — | — |
| `s_movreld_b32` | 0x30 | **No** | — | — |
| `s_movreld_b64` | 0x31 | **No** | — | — |
| `s_abs_i32` | 0x34 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34 |
| `s_andn1_saveexec_b64` | 0x37 | Yes | — | — |
| `s_orn1_saveexec_b64` | 0x38 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_andn1_wrexec_b64` | 0x39 | Yes | — | — |
| `s_andn2_wrexec_b64` | 0x3a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,225 |
| `s_bitreplicate_b64_b32` | 0x3b | Unproven: s_bitreplicate_b64_b32 needs proven ordinary scalar operands (not a wave mask) | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_and_saveexec_b32` | 0x3c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,451 |
| `s_or_saveexec_b32` | 0x3d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,642 |
| `s_xor_saveexec_b32` | 0x3e | Yes | — | — |
| `s_andn2_saveexec_b32` | 0x3f | Yes | — | — |
| `s_orn2_saveexec_b32` | 0x40 | Yes | — | — |
| `s_nand_saveexec_b32` | 0x41 | Yes | — | — |
| `s_nor_saveexec_b32` | 0x42 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_xnor_saveexec_b32` | 0x43 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_andn1_saveexec_b32` | 0x44 | Yes | — | — |
| `s_orn1_saveexec_b32` | 0x45 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_andn1_wrexec_b32` | 0x46 | Yes | — | — |
| `s_andn2_wrexec_b32` | 0x47 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,768 |
| `s_movrelsd_2_b32` | 0x49 | **No** | — | — |

## Scalar compare (SOPC)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `s_cmp_eq_i32` | 0x0 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_cmp_lg_i32` | 0x1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_cmp_gt_i32` | 0x2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,051 |
| `s_cmp_ge_i32` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 231 |
| `s_cmp_lt_i32` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,048 |
| `s_cmp_le_i32` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18 |
| `s_cmp_eq_u32` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 35,260 |
| `s_cmp_lg_u32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 31,249 |
| `s_cmp_gt_u32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 21,049 |
| `s_cmp_ge_u32` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,837 |
| `s_cmp_lt_u32` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,420 |
| `s_cmp_le_u32` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,449 |
| `s_bitcmp0_b32` | 0xc | Yes | — | — |
| `s_bitcmp1_b32` | 0xd | Yes | — | — |
| `s_bitcmp0_b64` | 0xe | Unproven: s_bitcmp0_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_bitcmp1_b64` | 0xf | Unproven: s_bitcmp1_b64 needs proven ordinary scalar operands (not a wave mask) | — | — |
| `s_cmp_eq_u64` | 0x12 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 448 |
| `s_cmp_lg_u64` | 0x13 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |

## Program control (SOPP)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `s_nop` | 0x0 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 38,195 |
| `s_endpgm` | 0x1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 25,450 |
| `s_branch` | 0x2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 29,000 |
| `s_wakeup` | 0x3 | **No** | — | — |
| `s_cbranch_scc0` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 26,500 |
| `s_cbranch_scc1` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 29,313 |
| `s_cbranch_vccz` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,900 |
| `s_cbranch_vccnz` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 24,067 |
| `s_cbranch_execz` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,496 |
| `s_cbranch_execnz` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,565 |
| `s_barrier` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 658 |
| `s_setkill` | 0xb | **No** | — | — |
| `s_waitcnt` | 0xc | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 44,086 |
| `s_sethalt` | 0xd | **No** | — | — |
| `s_sleep` | 0xe | **No** | — | — |
| `s_setprio` | 0xf | **No** | — | — |
| `s_sendmsg` | 0x10 | Unproven: s_sendmsg outside a geometry shader | — | — |
| `s_sendmsghalt` | 0x11 | **No** | — | — |
| `s_trap` | 0x12 | **No** | — | — |
| `s_icache_inv` | 0x13 | **No** | — | — |
| `s_incperflevel` | 0x14 | **No** | — | — |
| `s_decperflevel` | 0x15 | **No** | — | — |
| `s_ttracedata` | 0x16 | **No** | — | — |
| `s_cbranch_cdbgsys` | 0x17 | **No** | — | — |
| `s_cbranch_cdbguser` | 0x18 | **No** | — | — |
| `s_cbranch_cdbgsys_or_user` | 0x19 | **No** | — | — |
| `s_cbranch_cdbgsys_and_user` | 0x1a | **No** | — | — |
| `s_endpgm_saved` | 0x1b | **No** | — | — |
| `s_endpgm_ordered_ps_done` | 0x1e | **No** | — | — |
| `s_code_end` | 0x1f | Nothing to emit | — | — |
| `s_inst_prefetch` | 0x20 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 44,348 |
| `s_clause` | 0x21 | Nothing to emit | — | — |
| `s_wait_idle` | 0x22 | **No** | — | — |
| `s_waitcnt_depctr` | 0x23 | Nothing to emit | — | — |
| `s_round_mode` | 0x24 | Yes | — | — |
| `s_denorm_mode` | 0x25 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `s_ttracedata_imm` | 0x28 | **No** | — | — |

## Scalar memory (SMEM)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `s_load_dword` | 0x0 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 841 |
| `s_load_dwordx2` | 0x1 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 158 |
| `s_load_dwordx4` | 0x2 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 43,821 |
| `s_load_dwordx8` | 0x3 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 30,350 |
| `s_load_dwordx16` | 0x4 | Nothing to emit | — | — |
| `s_scratch_load_dword` | 0x5 | **No** | — | — |
| `s_scratch_load_dwordx2` | 0x6 | **No** | — | — |
| `s_scratch_load_dwordx4` | 0x7 | **No** | — | — |
| `s_buffer_load_dword` | 0x8 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 40,775 |
| `s_buffer_load_dwordx2` | 0x9 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 36,397 |
| `s_buffer_load_dwordx4` | 0xa | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 38,501 |
| `s_buffer_load_dwordx8` | 0xb | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 25,251 |
| `s_buffer_load_dwordx16` | 0xc | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 25,005 |
| `s_gl1_inv` | 0x1f | **No** | — | — |
| `s_dcache_inv` | 0x20 | **No** | — | — |
| `s_memtime` | 0x24 | **No** | — | — |
| `s_memrealtime` | 0x25 | **No** | — | — |
| `s_atc_probe` | 0x26 | **No** | — | — |
| `s_atc_probe_buffer` | 0x27 | **No** | — | — |
| `s_get_waveid_in_workgroup` | 0x2a | **No** | — | — |

## Vector, two operands (VOP2)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `v_cndmask_b32` | 0x1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 38,680 |
| `v_dot2c_f32_f16` | 0x2 | **No** | — | — |
| `v_add_f32` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 39,754 |
| `v_sub_f32` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 36,106 |
| `v_subrev_f32` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 23,601 |
| `v_mac_legacy_f32` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mul_legacy_f32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 42,291 |
| `v_mul_f32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 17,824 |
| `v_mul_i32_i24` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |
| `v_mul_hi_i32_i24` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mul_u32_u24` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,244 |
| `v_mul_hi_u32_u24` | 0xc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_dot4c_i32_i8` | 0xd | Yes | — | — |
| `v_min_f32` | 0xf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 19,163 |
| `v_max_f32` | 0x10 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,940 |
| `v_min_i32` | 0x11 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,833 |
| `v_max_i32` | 0x12 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,915 |
| `v_min_u32` | 0x13 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,527 |
| `v_max_u32` | 0x14 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,591 |
| `v_lshrrev_b32` | 0x16 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 31,634 |
| `v_ashrrev_i32` | 0x18 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 3,702 |
| `v_lshlrev_b32` | 0x1a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34,179 |
| `v_and_b32` | 0x1b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 36,158 |
| `v_or_b32` | 0x1c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,176 |
| `v_xor_b32` | 0x1d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,671 |
| `v_xnor_b32` | 0x1e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6 |
| `v_mac_f32` | 0x1f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,311 |
| `v_madmk_f32` | 0x20 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_madak_f32` | 0x21 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8,938 |
| `v_add_nc_u32` | 0x25 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34,893 |
| `v_sub_nc_u32` | 0x26 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 17,696 |
| `v_subrev_nc_u32` | 0x27 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,503 |
| `v_add_co_ci_u32` | 0x28 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,782 |
| `v_sub_co_ci_u32` | 0x29 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 24 |
| `v_subrev_co_ci_u32` | 0x2a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 96 |
| `v_fmac_f32` | 0x2b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 40,834 |
| `v_fmamk_f32` | 0x2c | Yes | — | — |
| `v_fmaak_f32` | 0x2d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,208 |
| `v_cvt_pkrtz_f16_f32` | 0x2f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12,303 |
| `v_add_f16` | 0x32 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 108 |
| `v_sub_f16` | 0x33 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 108 |
| `v_subrev_f16` | 0x34 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mul_f16` | 0x35 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 212 |
| `v_fmac_f16` | 0x36 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 284 |
| `v_fmamk_f16` | 0x37 | Yes | — | — |
| `v_fmaak_f16` | 0x38 | Yes | — | — |
| `v_max_f16` | 0x39 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |
| `v_min_f16` | 0x3a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4 |
| `v_ldexp_f16` | 0x3b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_fmac_f16` | 0x3c | Yes | — | — |

## Vector, one operand (VOP1)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `v_nop` | 0x0 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 44,348 |
| `v_mov_b32` | 0x1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 42,133 |
| `v_readfirstlane_b32` | 0x2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 31,031 |
| `v_cvt_i32_f64` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_f64_i32` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_f32_i32` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 20,623 |
| `v_cvt_f32_u32` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,736 |
| `v_cvt_u32_f32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 30,617 |
| `v_cvt_i32_f32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,157 |
| `v_cvt_f16_f32` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12,430 |
| `v_cvt_f32_f16` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 17,410 |
| `v_cvt_rpi_i32_f32` | 0xc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_flr_i32_f32` | 0xd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_off_f32_i4` | 0xe | Yes | — | — |
| `v_cvt_f32_f64` | 0xf | Yes | — | — |
| `v_cvt_f64_f32` | 0x10 | Yes | — | — |
| `v_cvt_f32_ubyte0` | 0x11 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 30,666 |
| `v_cvt_f32_ubyte1` | 0x12 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 25,905 |
| `v_cvt_f32_ubyte2` | 0x13 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,394 |
| `v_cvt_f32_ubyte3` | 0x14 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 15,414 |
| `v_cvt_u32_f64` | 0x15 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_f64_u32` | 0x16 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_trunc_f64` | 0x17 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_ceil_f64` | 0x18 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_rndne_f64` | 0x19 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_floor_f64` | 0x1a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pipeflush` | 0x1b | **No** | — | — |
| `v_fract_f32` | 0x20 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 21,273 |
| `v_trunc_f32` | 0x21 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 26,964 |
| `v_ceil_f32` | 0x22 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,487 |
| `v_rndne_f32` | 0x23 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,248 |
| `v_floor_f32` | 0x24 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,260 |
| `v_exp_f32` | 0x25 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,838 |
| `v_log_f32` | 0x27 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,467 |
| `v_rcp_f32` | 0x2a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 30,995 |
| `v_rcp_iflag_f32` | 0x2b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,447 |
| `v_rsq_f32` | 0x2e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 29,517 |
| `v_rcp_f64` | 0x2f | **No** | — | — |
| `v_rsq_f64` | 0x31 | **No** | — | — |
| `v_sqrt_f32` | 0x33 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 26,602 |
| `v_sqrt_f64` | 0x34 | **No** | — | — |
| `v_sin_f32` | 0x35 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 21,597 |
| `v_cos_f32` | 0x36 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22,422 |
| `v_not_b32` | 0x37 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 206 |
| `v_bfrev_b32` | 0x38 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,386 |
| `v_ffbh_u32` | 0x39 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 112 |
| `v_ffbl_b32` | 0x3a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,912 |
| `v_ffbh_i32` | 0x3b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6 |
| `v_frexp_exp_i32_f64` | 0x3c | Yes | — | — |
| `v_frexp_mant_f64` | 0x3d | Yes | — | — |
| `v_fract_f64` | 0x3e | Yes | — | — |
| `v_frexp_exp_i32_f32` | 0x3f | Yes | — | — |
| `v_frexp_mant_f32` | 0x40 | Yes | — | — |
| `v_clrexcp` | 0x41 | **No** | — | — |
| `v_movreld_b32` | 0x42 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34 |
| `v_movrels_b32` | 0x43 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 118 |
| `v_movrelsd_b32` | 0x44 | Yes | — | — |
| `v_movrelsd_2_b32` | 0x48 | **No** | — | — |
| `v_cvt_f16_u16` | 0x50 | Yes | — | — |
| `v_cvt_f16_i16` | 0x51 | Yes | — | — |
| `v_cvt_u16_f16` | 0x52 | Yes | — | — |
| `v_cvt_i16_f16` | 0x53 | Yes | — | — |
| `v_rcp_f16` | 0x54 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 90 |
| `v_sqrt_f16` | 0x55 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8 |
| `v_rsq_f16` | 0x56 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 190 |
| `v_log_f16` | 0x57 | **No** | — | — |
| `v_exp_f16` | 0x58 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |
| `v_frexp_mant_f16` | 0x59 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_frexp_exp_i16_f16` | 0x5a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_floor_f16` | 0x5b | Yes | — | — |
| `v_ceil_f16` | 0x5c | Yes | — | — |
| `v_trunc_f16` | 0x5d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_rndne_f16` | 0x5e | Yes | — | — |
| `v_fract_f16` | 0x5f | Yes | — | — |
| `v_sin_f16` | 0x60 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cos_f16` | 0x61 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_sat_pk_u8_i16` | 0x62 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_norm_i16_f16` | 0x63 | Yes | — | — |
| `v_cvt_norm_u16_f16` | 0x64 | Yes | — | — |
| `v_swap_b32` | 0x65 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 74 |
| `v_swaprel_b32` | 0x68 | **No** | — | — |

## Vector compare (VOPC)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `v_cmp_f_f32` | 0x0 | Yes | — | — |
| `v_cmp_lt_f32` | 0x1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 27,901 |
| `v_cmp_eq_f32` | 0x2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 20,275 |
| `v_cmp_le_f32` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 28,884 |
| `v_cmp_gt_f32` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 25,724 |
| `v_cmp_lg_f32` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,895 |
| `v_cmp_ge_f32` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,307 |
| `v_cmp_o_f32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_u_f32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nge_f32` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,145 |
| `v_cmp_nlg_f32` | 0xa | Yes | — | — |
| `v_cmp_ngt_f32` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,166 |
| `v_cmp_nle_f32` | 0xc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,655 |
| `v_cmp_neq_f32` | 0xd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 7,195 |
| `v_cmp_nlt_f32` | 0xe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,606 |
| `v_cmp_tru_f32` | 0xf | Yes | — | — |
| `v_cmpx_f_f32` | 0x10 | Yes | — | — |
| `v_cmpx_lt_f32` | 0x11 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,177 |
| `v_cmpx_eq_f32` | 0x12 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,829 |
| `v_cmpx_le_f32` | 0x13 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,909 |
| `v_cmpx_gt_f32` | 0x14 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 20,029 |
| `v_cmpx_lg_f32` | 0x15 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,860 |
| `v_cmpx_ge_f32` | 0x16 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,272 |
| `v_cmpx_o_f32` | 0x17 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_u_f32` | 0x18 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nge_f32` | 0x19 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 519 |
| `v_cmpx_nlg_f32` | 0x1a | Yes | — | — |
| `v_cmpx_ngt_f32` | 0x1b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 344 |
| `v_cmpx_nle_f32` | 0x1c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 762 |
| `v_cmpx_neq_f32` | 0x1d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,895 |
| `v_cmpx_nlt_f32` | 0x1e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 46 |
| `v_cmpx_tru_f32` | 0x1f | Yes | — | — |
| `v_cmp_f_f64` | 0x20 | Yes | — | — |
| `v_cmp_lt_f64` | 0x21 | Yes | — | — |
| `v_cmp_eq_f64` | 0x22 | Yes | — | — |
| `v_cmp_le_f64` | 0x23 | Yes | — | — |
| `v_cmp_gt_f64` | 0x24 | Yes | — | — |
| `v_cmp_lg_f64` | 0x25 | Yes | — | — |
| `v_cmp_ge_f64` | 0x26 | Yes | — | — |
| `v_cmp_o_f64` | 0x27 | Yes | — | — |
| `v_cmp_u_f64` | 0x28 | Yes | — | — |
| `v_cmp_nge_f64` | 0x29 | Yes | — | — |
| `v_cmp_nlg_f64` | 0x2a | Yes | — | — |
| `v_cmp_ngt_f64` | 0x2b | Yes | — | — |
| `v_cmp_nle_f64` | 0x2c | Yes | — | — |
| `v_cmp_neq_f64` | 0x2d | Yes | — | — |
| `v_cmp_nlt_f64` | 0x2e | Yes | — | — |
| `v_cmp_tru_f64` | 0x2f | Yes | — | — |
| `v_cmpx_f_f64` | 0x30 | Yes | — | — |
| `v_cmpx_lt_f64` | 0x31 | Yes | — | — |
| `v_cmpx_eq_f64` | 0x32 | Yes | — | — |
| `v_cmpx_le_f64` | 0x33 | Yes | — | — |
| `v_cmpx_gt_f64` | 0x34 | Yes | — | — |
| `v_cmpx_lg_f64` | 0x35 | Yes | — | — |
| `v_cmpx_ge_f64` | 0x36 | Yes | — | — |
| `v_cmpx_o_f64` | 0x37 | Yes | — | — |
| `v_cmpx_u_f64` | 0x38 | Yes | — | — |
| `v_cmpx_nge_f64` | 0x39 | Yes | — | — |
| `v_cmpx_nlg_f64` | 0x3a | Yes | — | — |
| `v_cmpx_ngt_f64` | 0x3b | Yes | — | — |
| `v_cmpx_nle_f64` | 0x3c | Yes | — | — |
| `v_cmpx_neq_f64` | 0x3d | Yes | — | — |
| `v_cmpx_nlt_f64` | 0x3e | Yes | — | — |
| `v_cmpx_tru_f64` | 0x3f | Yes | — | — |
| `v_cmp_f_i32` | 0x80 | Yes | — | — |
| `v_cmp_lt_i32` | 0x81 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,905 |
| `v_cmp_eq_i32` | 0x82 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16 |
| `v_cmp_le_i32` | 0x83 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,551 |
| `v_cmp_gt_i32` | 0x84 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,077 |
| `v_cmp_ne_i32` | 0x85 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ge_i32` | 0x86 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 116 |
| `v_cmp_t_i32` | 0x87 | Yes | — | — |
| `v_cmp_class_f32` | 0x88 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,454 |
| `v_cmp_lt_i16` | 0x89 | Yes | — | — |
| `v_cmp_eq_i16` | 0x8a | Yes | — | — |
| `v_cmp_le_i16` | 0x8b | Yes | — | — |
| `v_cmp_gt_i16` | 0x8c | Yes | — | — |
| `v_cmp_ne_i16` | 0x8d | Yes | — | — |
| `v_cmp_ge_i16` | 0x8e | Yes | — | — |
| `v_cmp_class_f16` | 0x8f | Yes | — | — |
| `v_cmpx_f_i32` | 0x90 | Yes | — | — |
| `v_cmpx_lt_i32` | 0x91 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,173 |
| `v_cmpx_eq_i32` | 0x92 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 692 |
| `v_cmpx_le_i32` | 0x93 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 144 |
| `v_cmpx_gt_i32` | 0x94 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,728 |
| `v_cmpx_ne_i32` | 0x95 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ge_i32` | 0x96 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 339 |
| `v_cmpx_t_i32` | 0x97 | Yes | — | — |
| `v_cmpx_class_f32` | 0x98 | Yes | — | — |
| `v_cmpx_lt_i16` | 0x99 | Yes | — | — |
| `v_cmpx_eq_i16` | 0x9a | Yes | — | — |
| `v_cmpx_le_i16` | 0x9b | Yes | — | — |
| `v_cmpx_gt_i16` | 0x9c | Yes | — | — |
| `v_cmpx_ne_i16` | 0x9d | Yes | — | — |
| `v_cmpx_ge_i16` | 0x9e | Yes | — | — |
| `v_cmpx_class_f16` | 0x9f | Yes | — | — |
| `v_cmp_f_i64` | 0xa0 | Yes | — | — |
| `v_cmp_lt_i64` | 0xa1 | Yes | — | — |
| `v_cmp_eq_i64` | 0xa2 | Yes | — | — |
| `v_cmp_le_i64` | 0xa3 | Yes | — | — |
| `v_cmp_gt_i64` | 0xa4 | Yes | — | — |
| `v_cmp_ne_i64` | 0xa5 | Yes | — | — |
| `v_cmp_ge_i64` | 0xa6 | Yes | — | — |
| `v_cmp_t_i64` | 0xa7 | Yes | — | — |
| `v_cmp_class_f64` | 0xa8 | Yes | — | — |
| `v_cmp_lt_u16` | 0xa9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_eq_u16` | 0xaa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cmp_le_u16` | 0xab | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_gt_u16` | 0xac | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ne_u16` | 0xad | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ge_u16` | 0xae | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_f_i64` | 0xb0 | Yes | — | — |
| `v_cmpx_lt_i64` | 0xb1 | Yes | — | — |
| `v_cmpx_eq_i64` | 0xb2 | Yes | — | — |
| `v_cmpx_le_i64` | 0xb3 | Yes | — | — |
| `v_cmpx_gt_i64` | 0xb4 | Yes | — | — |
| `v_cmpx_ne_i64` | 0xb5 | Yes | — | — |
| `v_cmpx_ge_i64` | 0xb6 | Yes | — | — |
| `v_cmpx_t_i64` | 0xb7 | Yes | — | — |
| `v_cmpx_class_f64` | 0xb8 | Yes | — | — |
| `v_cmpx_lt_u16` | 0xb9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_eq_u16` | 0xba | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_le_u16` | 0xbb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_gt_u16` | 0xbc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ne_u16` | 0xbd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cmpx_ge_u16` | 0xbe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_f_u32` | 0xc0 | Yes | — | — |
| `v_cmp_lt_u32` | 0xc1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,432 |
| `v_cmp_eq_u32` | 0xc2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,482 |
| `v_cmp_le_u32` | 0xc3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,922 |
| `v_cmp_gt_u32` | 0xc4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 7,550 |
| `v_cmp_ne_u32` | 0xc5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22,848 |
| `v_cmp_ge_u32` | 0xc6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,943 |
| `v_cmp_t_u32` | 0xc7 | Yes | — | — |
| `v_cmp_f_f16` | 0xc8 | Yes | — | — |
| `v_cmp_lt_f16` | 0xc9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 588 |
| `v_cmp_eq_f16` | 0xca | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32 |
| `v_cmp_le_f16` | 0xcb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8 |
| `v_cmp_gt_f16` | 0xcc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 369 |
| `v_cmp_lg_f16` | 0xcd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ge_f16` | 0xce | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_o_f16` | 0xcf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_f_u32` | 0xd0 | Yes | — | — |
| `v_cmpx_lt_u32` | 0xd1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,953 |
| `v_cmpx_eq_u32` | 0xd2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 21,175 |
| `v_cmpx_le_u32` | 0xd3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 282 |
| `v_cmpx_gt_u32` | 0xd4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22,295 |
| `v_cmpx_ne_u32` | 0xd5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,859 |
| `v_cmpx_ge_u32` | 0xd6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 124 |
| `v_cmpx_t_u32` | 0xd7 | Yes | — | — |
| `v_cmpx_f_f16` | 0xd8 | Yes | — | — |
| `v_cmpx_lt_f16` | 0xd9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 286 |
| `v_cmpx_eq_f16` | 0xda | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22 |
| `v_cmpx_le_f16` | 0xdb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_gt_f16` | 0xdc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 62 |
| `v_cmpx_lg_f16` | 0xdd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ge_f16` | 0xde | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34 |
| `v_cmpx_o_f16` | 0xdf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_f_u64` | 0xe0 | Yes | — | — |
| `v_cmp_lt_u64` | 0xe1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_eq_u64` | 0xe2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 436 |
| `v_cmp_le_u64` | 0xe3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_gt_u64` | 0xe4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ne_u64` | 0xe5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 448 |
| `v_cmp_ge_u64` | 0xe6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_t_u64` | 0xe7 | Yes | — | — |
| `v_cmp_u_f16` | 0xe8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nge_f16` | 0xe9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nlg_f16` | 0xea | Yes | — | — |
| `v_cmp_ngt_f16` | 0xeb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nle_f16` | 0xec | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_neq_f16` | 0xed | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 219 |
| `v_cmp_nlt_f16` | 0xee | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16 |
| `v_cmp_tru_f16` | 0xef | Yes | — | — |
| `v_cmpx_f_u64` | 0xf0 | Yes | — | — |
| `v_cmpx_lt_u64` | 0xf1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_eq_u64` | 0xf2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cmpx_le_u64` | 0xf3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_gt_u64` | 0xf4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ne_u64` | 0xf5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 76 |
| `v_cmpx_ge_u64` | 0xf6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_t_u64` | 0xf7 | Yes | — | — |
| `v_cmpx_u_f16` | 0xf8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nge_f16` | 0xf9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nlg_f16` | 0xfa | Yes | — | — |
| `v_cmpx_ngt_f16` | 0xfb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nle_f16` | 0xfc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_neq_f16` | 0xfd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nlt_f16` | 0xfe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_tru_f16` | 0xff | Yes | — | — |

## Vector, three operands (VOP3 and VOP3P)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `v_cmp_f_f32` | 0x0 | Yes | — | — |
| `v_cmp_lt_f32` | 0x1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 27,901 |
| `v_cmp_eq_f32` | 0x2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 20,275 |
| `v_cmp_le_f32` | 0x3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 28,884 |
| `v_cmp_gt_f32` | 0x4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 25,724 |
| `v_cmp_lg_f32` | 0x5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,895 |
| `v_cmp_ge_f32` | 0x6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,307 |
| `v_cmp_o_f32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_u_f32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nge_f32` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,145 |
| `v_cmp_nlg_f32` | 0xa | Yes | — | — |
| `v_cmp_ngt_f32` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,166 |
| `v_cmp_nle_f32` | 0xc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,655 |
| `v_cmp_neq_f32` | 0xd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 7,195 |
| `v_cmp_nlt_f32` | 0xe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,606 |
| `v_cmp_tru_f32` | 0xf | Yes | — | — |
| `v_cmpx_f_f32` | 0x10 | Yes | — | — |
| `v_cmpx_lt_f32` | 0x11 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,177 |
| `v_cmpx_eq_f32` | 0x12 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,829 |
| `v_cmpx_le_f32` | 0x13 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,909 |
| `v_cmpx_gt_f32` | 0x14 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 20,029 |
| `v_cmpx_lg_f32` | 0x15 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,860 |
| `v_cmpx_ge_f32` | 0x16 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,272 |
| `v_cmpx_o_f32` | 0x17 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_u_f32` | 0x18 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nge_f32` | 0x19 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 519 |
| `v_cmpx_nlg_f32` | 0x1a | Yes | — | — |
| `v_cmpx_ngt_f32` | 0x1b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 344 |
| `v_cmpx_nle_f32` | 0x1c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 762 |
| `v_cmpx_neq_f32` | 0x1d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,895 |
| `v_cmpx_nlt_f32` | 0x1e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 46 |
| `v_cmpx_tru_f32` | 0x1f | Yes | — | — |
| `v_cmp_f_f64` | 0x20 | Yes | — | — |
| `v_cmp_lt_f64` | 0x21 | Yes | — | — |
| `v_cmp_eq_f64` | 0x22 | Yes | — | — |
| `v_cmp_le_f64` | 0x23 | Yes | — | — |
| `v_cmp_gt_f64` | 0x24 | Yes | — | — |
| `v_cmp_lg_f64` | 0x25 | Yes | — | — |
| `v_cmp_ge_f64` | 0x26 | Yes | — | — |
| `v_cmp_o_f64` | 0x27 | Yes | — | — |
| `v_cmp_u_f64` | 0x28 | Yes | — | — |
| `v_cmp_nge_f64` | 0x29 | Yes | — | — |
| `v_cmp_nlg_f64` | 0x2a | Yes | — | — |
| `v_cmp_ngt_f64` | 0x2b | Yes | — | — |
| `v_cmp_nle_f64` | 0x2c | Yes | — | — |
| `v_cmp_neq_f64` | 0x2d | Yes | — | — |
| `v_cmp_nlt_f64` | 0x2e | Yes | — | — |
| `v_cmp_tru_f64` | 0x2f | Yes | — | — |
| `v_cmpx_f_f64` | 0x30 | Yes | — | — |
| `v_cmpx_lt_f64` | 0x31 | Yes | — | — |
| `v_cmpx_eq_f64` | 0x32 | Yes | — | — |
| `v_cmpx_le_f64` | 0x33 | Yes | — | — |
| `v_cmpx_gt_f64` | 0x34 | Yes | — | — |
| `v_cmpx_lg_f64` | 0x35 | Yes | — | — |
| `v_cmpx_ge_f64` | 0x36 | Yes | — | — |
| `v_cmpx_o_f64` | 0x37 | Yes | — | — |
| `v_cmpx_u_f64` | 0x38 | Yes | — | — |
| `v_cmpx_nge_f64` | 0x39 | Yes | — | — |
| `v_cmpx_nlg_f64` | 0x3a | Yes | — | — |
| `v_cmpx_ngt_f64` | 0x3b | Yes | — | — |
| `v_cmpx_nle_f64` | 0x3c | Yes | — | — |
| `v_cmpx_neq_f64` | 0x3d | Yes | — | — |
| `v_cmpx_nlt_f64` | 0x3e | Yes | — | — |
| `v_cmpx_tru_f64` | 0x3f | Yes | — | — |
| `v_cmp_f_i32` | 0x80 | Yes | — | — |
| `v_cmp_lt_i32` | 0x81 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,905 |
| `v_cmp_eq_i32` | 0x82 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16 |
| `v_cmp_le_i32` | 0x83 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,551 |
| `v_cmp_gt_i32` | 0x84 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,077 |
| `v_cmp_ne_i32` | 0x85 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ge_i32` | 0x86 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 116 |
| `v_cmp_t_i32` | 0x87 | Yes | — | — |
| `v_cmp_class_f32` | 0x88 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,454 |
| `v_cmp_lt_i16` | 0x89 | Yes | — | — |
| `v_cmp_eq_i16` | 0x8a | Yes | — | — |
| `v_cmp_le_i16` | 0x8b | Yes | — | — |
| `v_cmp_gt_i16` | 0x8c | Yes | — | — |
| `v_cmp_ne_i16` | 0x8d | Yes | — | — |
| `v_cmp_ge_i16` | 0x8e | Yes | — | — |
| `v_cmp_class_f16` | 0x8f | Yes | — | — |
| `v_cmpx_f_i32` | 0x90 | Yes | — | — |
| `v_cmpx_lt_i32` | 0x91 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,173 |
| `v_cmpx_eq_i32` | 0x92 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 692 |
| `v_cmpx_le_i32` | 0x93 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 144 |
| `v_cmpx_gt_i32` | 0x94 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,728 |
| `v_cmpx_ne_i32` | 0x95 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ge_i32` | 0x96 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 339 |
| `v_cmpx_t_i32` | 0x97 | Yes | — | — |
| `v_cmpx_class_f32` | 0x98 | Yes | — | — |
| `v_cmpx_lt_i16` | 0x99 | Yes | — | — |
| `v_cmpx_eq_i16` | 0x9a | Yes | — | — |
| `v_cmpx_le_i16` | 0x9b | Yes | — | — |
| `v_cmpx_gt_i16` | 0x9c | Yes | — | — |
| `v_cmpx_ne_i16` | 0x9d | Yes | — | — |
| `v_cmpx_ge_i16` | 0x9e | Yes | — | — |
| `v_cmpx_class_f16` | 0x9f | Yes | — | — |
| `v_cmp_f_i64` | 0xa0 | Yes | — | — |
| `v_cmp_lt_i64` | 0xa1 | Yes | — | — |
| `v_cmp_eq_i64` | 0xa2 | Yes | — | — |
| `v_cmp_le_i64` | 0xa3 | Yes | — | — |
| `v_cmp_gt_i64` | 0xa4 | Yes | — | — |
| `v_cmp_ne_i64` | 0xa5 | Yes | — | — |
| `v_cmp_ge_i64` | 0xa6 | Yes | — | — |
| `v_cmp_t_i64` | 0xa7 | Yes | — | — |
| `v_cmp_class_f64` | 0xa8 | Yes | — | — |
| `v_cmp_lt_u16` | 0xa9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_eq_u16` | 0xaa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cmp_le_u16` | 0xab | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_gt_u16` | 0xac | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ne_u16` | 0xad | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ge_u16` | 0xae | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_f_i64` | 0xb0 | Yes | — | — |
| `v_cmpx_lt_i64` | 0xb1 | Yes | — | — |
| `v_cmpx_eq_i64` | 0xb2 | Yes | — | — |
| `v_cmpx_le_i64` | 0xb3 | Yes | — | — |
| `v_cmpx_gt_i64` | 0xb4 | Yes | — | — |
| `v_cmpx_ne_i64` | 0xb5 | Yes | — | — |
| `v_cmpx_ge_i64` | 0xb6 | Yes | — | — |
| `v_cmpx_t_i64` | 0xb7 | Yes | — | — |
| `v_cmpx_class_f64` | 0xb8 | Yes | — | — |
| `v_cmpx_lt_u16` | 0xb9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_eq_u16` | 0xba | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_le_u16` | 0xbb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_gt_u16` | 0xbc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ne_u16` | 0xbd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cmpx_ge_u16` | 0xbe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_f_u32` | 0xc0 | Yes | — | — |
| `v_cmp_lt_u32` | 0xc1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,432 |
| `v_cmp_eq_u32` | 0xc2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,482 |
| `v_cmp_le_u32` | 0xc3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,922 |
| `v_cmp_gt_u32` | 0xc4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 7,550 |
| `v_cmp_ne_u32` | 0xc5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22,848 |
| `v_cmp_ge_u32` | 0xc6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 9,943 |
| `v_cmp_t_u32` | 0xc7 | Yes | — | — |
| `v_cmp_f_f16` | 0xc8 | Yes | — | — |
| `v_cmp_lt_f16` | 0xc9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 588 |
| `v_cmp_eq_f16` | 0xca | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32 |
| `v_cmp_le_f16` | 0xcb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8 |
| `v_cmp_gt_f16` | 0xcc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 369 |
| `v_cmp_lg_f16` | 0xcd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ge_f16` | 0xce | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_o_f16` | 0xcf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_f_u32` | 0xd0 | Yes | — | — |
| `v_cmpx_lt_u32` | 0xd1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,953 |
| `v_cmpx_eq_u32` | 0xd2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 21,175 |
| `v_cmpx_le_u32` | 0xd3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 282 |
| `v_cmpx_gt_u32` | 0xd4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22,295 |
| `v_cmpx_ne_u32` | 0xd5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,859 |
| `v_cmpx_ge_u32` | 0xd6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 124 |
| `v_cmpx_t_u32` | 0xd7 | Yes | — | — |
| `v_cmpx_f_f16` | 0xd8 | Yes | — | — |
| `v_cmpx_lt_f16` | 0xd9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 286 |
| `v_cmpx_eq_f16` | 0xda | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22 |
| `v_cmpx_le_f16` | 0xdb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_gt_f16` | 0xdc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 62 |
| `v_cmpx_lg_f16` | 0xdd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ge_f16` | 0xde | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34 |
| `v_cmpx_o_f16` | 0xdf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_f_u64` | 0xe0 | Yes | — | — |
| `v_cmp_lt_u64` | 0xe1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_eq_u64` | 0xe2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 436 |
| `v_cmp_le_u64` | 0xe3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_gt_u64` | 0xe4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_ne_u64` | 0xe5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 448 |
| `v_cmp_ge_u64` | 0xe6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_t_u64` | 0xe7 | Yes | — | — |
| `v_cmp_u_f16` | 0xe8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nge_f16` | 0xe9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nlg_f16` | 0xea | Yes | — | — |
| `v_cmp_ngt_f16` | 0xeb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_nle_f16` | 0xec | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmp_neq_f16` | 0xed | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 219 |
| `v_cmp_nlt_f16` | 0xee | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16 |
| `v_cmp_tru_f16` | 0xef | Yes | — | — |
| `v_cmpx_f_u64` | 0xf0 | Yes | — | — |
| `v_cmpx_lt_u64` | 0xf1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_eq_u64` | 0xf2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cmpx_le_u64` | 0xf3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_gt_u64` | 0xf4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_ne_u64` | 0xf5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 76 |
| `v_cmpx_ge_u64` | 0xf6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_t_u64` | 0xf7 | Yes | — | — |
| `v_cmpx_u_f16` | 0xf8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nge_f16` | 0xf9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nlg_f16` | 0xfa | Yes | — | — |
| `v_cmpx_ngt_f16` | 0xfb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nle_f16` | 0xfc | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_neq_f16` | 0xfd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_nlt_f16` | 0xfe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cmpx_tru_f16` | 0xff | Yes | — | — |
| `v_cndmask_b32` | 0x101 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 38,680 |
| `v_add_f32` | 0x103 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 39,754 |
| `v_sub_f32` | 0x104 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 36,106 |
| `v_subrev_f32` | 0x105 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 23,601 |
| `v_mac_legacy_f32` | 0x106 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mul_legacy_f32` | 0x107 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 42,291 |
| `v_mul_f32` | 0x108 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 17,824 |
| `v_mul_i32_i24` | 0x109 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |
| `v_mul_hi_i32_i24` | 0x10a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mul_u32_u24` | 0x10b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,244 |
| `v_mul_hi_u32_u24` | 0x10c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_min_f32` | 0x10f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 19,163 |
| `v_max_f32` | 0x110 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,940 |
| `v_min_i32` | 0x111 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,833 |
| `v_max_i32` | 0x112 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,915 |
| `v_min_u32` | 0x113 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,527 |
| `v_max_u32` | 0x114 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,591 |
| `v_lshrrev_b32` | 0x116 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 31,634 |
| `v_ashrrev_i32` | 0x118 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 3,702 |
| `v_lshlrev_b32` | 0x11a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34,179 |
| `v_and_b32` | 0x11b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 36,158 |
| `v_or_b32` | 0x11c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,176 |
| `v_xor_b32` | 0x11d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,671 |
| `v_xnor_b32` | 0x11e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6 |
| `v_mac_f32` | 0x11f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,311 |
| `v_add_nc_u32` | 0x125 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34,893 |
| `v_sub_nc_u32` | 0x126 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 17,696 |
| `v_subrev_nc_u32` | 0x127 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,503 |
| `v_add_co_ci_u32` | 0x128 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,782 |
| `v_sub_co_ci_u32` | 0x129 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 24 |
| `v_subrev_co_ci_u32` | 0x12a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 96 |
| `v_fmac_f32` | 0x12b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 40,834 |
| `v_cvt_pkrtz_f16_f32` | 0x12f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12,303 |
| `v_add_f16` | 0x132 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 108 |
| `v_sub_f16` | 0x133 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 108 |
| `v_subrev_f16` | 0x134 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mul_f16` | 0x135 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 212 |
| `v_fmac_f16` | 0x136 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 284 |
| `v_max_f16` | 0x139 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |
| `v_min_f16` | 0x13a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4 |
| `v_ldexp_f16` | 0x13b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mad_legacy_f32` | 0x140 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,466 |
| `v_mad_f32` | 0x141 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,229 |
| `v_mad_i32_i24` | 0x142 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 52 |
| `v_mad_u32_u24` | 0x143 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 24,150 |
| `v_cubeid_f32` | 0x144 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,149 |
| `v_cubesc_f32` | 0x145 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,166 |
| `v_cubetc_f32` | 0x146 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,166 |
| `v_cubema_f32` | 0x147 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,166 |
| `v_bfe_u32` | 0x148 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 26,322 |
| `v_bfe_i32` | 0x149 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 40 |
| `v_bfi_b32` | 0x14a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,234 |
| `v_fma_f32` | 0x14b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 38,503 |
| `v_fma_f64` | 0x14c | **No** | — | — |
| `v_lerp_u8` | 0x14d | Yes | — | — |
| `v_alignbit_b32` | 0x14e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,307 |
| `v_alignbyte_b32` | 0x14f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18 |
| `v_mullit_f32` | 0x150 | Yes | — | — |
| `v_min3_f32` | 0x151 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8,297 |
| `v_min3_i32` | 0x152 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_min3_u32` | 0x153 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 84 |
| `v_max3_f32` | 0x154 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,492 |
| `v_max3_i32` | 0x155 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_max3_u32` | 0x156 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 78 |
| `v_med3_f32` | 0x157 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,315 |
| `v_med3_i32` | 0x158 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,503 |
| `v_med3_u32` | 0x159 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 174 |
| `v_sad_u8` | 0x15a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_sad_hi_u8` | 0x15b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_sad_u16` | 0x15c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_sad_u32` | 0x15d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_pk_u8_f32` | 0x15e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 686 |
| `v_div_fixup_f32` | 0x15f | **No** | — | — |
| `v_div_fixup_f64` | 0x160 | **No** | — | — |
| `v_add_f64` | 0x164 | **No** | — | — |
| `v_mul_f64` | 0x165 | **No** | — | — |
| `v_min_f64` | 0x166 | Yes | — | — |
| `v_max_f64` | 0x167 | Yes | — | — |
| `v_ldexp_f64` | 0x168 | Yes | — | — |
| `v_mul_lo_u32` | 0x169 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 19,901 |
| `v_mul_hi_u32` | 0x16a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,266 |
| `v_mul_hi_i32` | 0x16c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 160 |
| `v_div_scale_f32` | 0x16d | **No** | — | — |
| `v_div_scale_f64` | 0x16e | **No** | — | — |
| `v_div_fmas_f32` | 0x16f | **No** | — | — |
| `v_div_fmas_f64` | 0x170 | **No** | — | — |
| `v_msad_u8` | 0x171 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_qsad_pk_u16_u8` | 0x172 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mqsad_pk_u16_u8` | 0x173 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_trig_preop_f64` | 0x174 | **No** | — | — |
| `v_mqsad_u32_u8` | 0x175 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mad_u64_u32` | 0x176 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mad_i64_i32` | 0x177 | **No** | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_xor3_b32` | 0x178 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_nop` | 0x180 | Nothing to emit | AMD 1002:13c0, NVIDIA 10de:2705 | 44,348 |
| `v_mov_b32` | 0x181 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 42,133 |
| `v_readfirstlane_b32` | 0x182 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 31,031 |
| `v_cvt_i32_f64` | 0x183 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_f64_i32` | 0x184 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_f32_i32` | 0x185 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 20,623 |
| `v_cvt_f32_u32` | 0x186 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32,736 |
| `v_cvt_u32_f32` | 0x187 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 30,617 |
| `v_cvt_i32_f32` | 0x188 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,157 |
| `v_cvt_f16_f32` | 0x18a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12,430 |
| `v_cvt_f32_f16` | 0x18b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 17,410 |
| `v_cvt_rpi_i32_f32` | 0x18c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_flr_i32_f32` | 0x18d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_off_f32_i4` | 0x18e | Yes | — | — |
| `v_cvt_f32_f64` | 0x18f | Yes | — | — |
| `v_cvt_f64_f32` | 0x190 | Yes | — | — |
| `v_cvt_f32_ubyte0` | 0x191 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 30,666 |
| `v_cvt_f32_ubyte1` | 0x192 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 25,905 |
| `v_cvt_f32_ubyte2` | 0x193 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 18,394 |
| `v_cvt_f32_ubyte3` | 0x194 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 15,414 |
| `v_cvt_u32_f64` | 0x195 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_f64_u32` | 0x196 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_trunc_f64` | 0x197 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_ceil_f64` | 0x198 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_rndne_f64` | 0x199 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_floor_f64` | 0x19a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pipeflush` | 0x19b | **No** | — | — |
| `v_fract_f32` | 0x1a0 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 21,273 |
| `v_trunc_f32` | 0x1a1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 26,964 |
| `v_ceil_f32` | 0x1a2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,487 |
| `v_rndne_f32` | 0x1a3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 11,248 |
| `v_floor_f32` | 0x1a4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 13,260 |
| `v_exp_f32` | 0x1a5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,838 |
| `v_log_f32` | 0x1a7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,467 |
| `v_rcp_f32` | 0x1aa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 30,995 |
| `v_rcp_iflag_f32` | 0x1ab | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 16,447 |
| `v_rsq_f32` | 0x1ae | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 29,517 |
| `v_rcp_f64` | 0x1af | **No** | — | — |
| `v_rsq_f64` | 0x1b1 | **No** | — | — |
| `v_sqrt_f32` | 0x1b3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 26,602 |
| `v_sqrt_f64` | 0x1b4 | **No** | — | — |
| `v_sin_f32` | 0x1b5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 21,597 |
| `v_cos_f32` | 0x1b6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22,422 |
| `v_not_b32` | 0x1b7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 206 |
| `v_bfrev_b32` | 0x1b8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4,386 |
| `v_ffbh_u32` | 0x1b9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 112 |
| `v_ffbl_b32` | 0x1ba | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,912 |
| `v_ffbh_i32` | 0x1bb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6 |
| `v_frexp_exp_i32_f64` | 0x1bc | Yes | — | — |
| `v_frexp_mant_f64` | 0x1bd | Yes | — | — |
| `v_fract_f64` | 0x1be | Yes | — | — |
| `v_frexp_exp_i32_f32` | 0x1bf | Yes | — | — |
| `v_frexp_mant_f32` | 0x1c0 | Yes | — | — |
| `v_clrexcp` | 0x1c1 | **No** | — | — |
| `v_movreld_b32` | 0x1c2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34 |
| `v_movrels_b32` | 0x1c3 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 118 |
| `v_movrelsd_b32` | 0x1c4 | Yes | — | — |
| `v_movrelsd_2_b32` | 0x1c8 | **No** | — | — |
| `v_cvt_f16_u16` | 0x1d0 | Yes | — | — |
| `v_cvt_f16_i16` | 0x1d1 | Yes | — | — |
| `v_cvt_u16_f16` | 0x1d2 | Yes | — | — |
| `v_cvt_i16_f16` | 0x1d3 | Yes | — | — |
| `v_rcp_f16` | 0x1d4 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 90 |
| `v_sqrt_f16` | 0x1d5 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8 |
| `v_rsq_f16` | 0x1d6 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 190 |
| `v_log_f16` | 0x1d7 | **No** | — | — |
| `v_exp_f16` | 0x1d8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |
| `v_frexp_mant_f16` | 0x1d9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_frexp_exp_i16_f16` | 0x1da | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_floor_f16` | 0x1db | Yes | — | — |
| `v_ceil_f16` | 0x1dc | Yes | — | — |
| `v_trunc_f16` | 0x1dd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_rndne_f16` | 0x1de | Yes | — | — |
| `v_fract_f16` | 0x1df | Yes | — | — |
| `v_sin_f16` | 0x1e0 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_cos_f16` | 0x1e1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_sat_pk_u8_i16` | 0x1e2 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_norm_i16_f16` | 0x1e3 | Yes | — | — |
| `v_cvt_norm_u16_f16` | 0x1e4 | Yes | — | — |
| `v_interp_p1_f32` | 0x200 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 13,513 |
| `v_interp_p2_f32` | 0x201 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 13,513 |
| `v_interp_mov_f32` | 0x202 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 4,326 |
| `v_lshlrev_b64` | 0x2ff | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 602 |
| `v_lshrrev_b64` | 0x300 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_ashrrev_i64` | 0x301 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_add_nc_u16` | 0x303 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_sub_nc_u16` | 0x304 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mul_lo_u16` | 0x305 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_lshrrev_b16` | 0x307 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 954 |
| `v_ashrrev_i16` | 0x308 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_max_u16` | 0x309 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_max_i16` | 0x30a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_min_u16` | 0x30b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_min_i16` | 0x30c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_add_nc_i16` | 0x30d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_sub_nc_i16` | 0x30e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_add_co_u32` | 0x30f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,696 |
| `v_sub_co_u32` | 0x310 | Yes | — | — |
| `v_pack_b32_f16` | 0x311 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 7,538 |
| `v_cvt_pknorm_i16_f16` | 0x312 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_pknorm_u16_f16` | 0x313 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_lshlrev_b16` | 0x314 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_subrev_co_u32` | 0x319 | Yes | — | — |
| `v_mad_u16` | 0x340 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_interp_p1ll_f16` | 0x342 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 5,435 |
| `v_interp_p1lv_f16` | 0x343 | Unproven: v_interp outside a pixel shader | — | — |
| `v_perm_b32` | 0x344 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 15,101 |
| `v_xad_u32` | 0x345 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 5,445 |
| `v_lshl_add_u32` | 0x346 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 29,238 |
| `v_add_lshl_u32` | 0x347 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2,703 |
| `v_fma_f16` | 0x34b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 230 |
| `v_min3_f16` | 0x351 | Yes | — | — |
| `v_min3_i16` | 0x352 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_min3_u16` | 0x353 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_max3_f16` | 0x354 | Yes | — | — |
| `v_max3_i16` | 0x355 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_max3_u16` | 0x356 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_med3_f16` | 0x357 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 22 |
| `v_med3_i16` | 0x358 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_med3_u16` | 0x359 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_interp_p2_f16` | 0x35a | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 5,435 |
| `v_mad_i16` | 0x35e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_div_fixup_f16` | 0x35f | **No** | — | — |
| `v_readlane_b32` | 0x360 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 6,097 |
| `v_writelane_b32` | 0x361 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 11 |
| `v_ldexp_f32` | 0x362 | Yes | — | — |
| `v_bfm_b32` | 0x363 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 732 |
| `v_bcnt_u32_b32` | 0x364 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 3,605 |
| `v_mbcnt_lo_u32_b32` | 0x365 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,802 |
| `v_mbcnt_hi_u32_b32` | 0x366 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,432 |
| `v_cvt_pknorm_i16_f32` | 0x368 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_pknorm_u16_f32` | 0x369 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_cvt_pk_u16_u32` | 0x36a | Yes | — | — |
| `v_cvt_pk_i16_i32` | 0x36b | Yes | — | — |
| `v_add3_u32` | 0x36d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 15,051 |
| `v_lshl_or_b32` | 0x36f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 17,445 |
| `v_and_or_b32` | 0x371 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14,892 |
| `v_or3_b32` | 0x372 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 27,559 |
| `v_mad_u32_u16` | 0x373 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_mad_i32_i16` | 0x375 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_sub_nc_i32` | 0x376 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_permlane16_b32` | 0x377 | **No** | — | — |
| `v_permlanex16_b32` | 0x378 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 6,101 |
| `v_add_nc_i32` | 0x37f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_mad_i16` | 0x400 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_mul_lo_u16` | 0x401 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_add_i16` | 0x402 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_sub_i16` | 0x403 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_lshlrev_b16` | 0x404 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_pk_lshrrev_b16` | 0x405 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `v_pk_ashrrev_i16` | 0x406 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_max_i16` | 0x407 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_min_i16` | 0x408 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_mad_u16` | 0x409 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_add_u16` | 0x40a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_sub_u16` | 0x40b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_max_u16` | 0x40c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_min_u16` | 0x40d | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `v_pk_fma_f16` | 0x40e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 156 |
| `v_pk_add_f16` | 0x40f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 174 |
| `v_pk_mul_f16` | 0x410 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 138 |
| `v_pk_min_f16` | 0x411 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 10 |
| `v_pk_max_f16` | 0x412 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 14 |
| `v_dot2_f32_f16` | 0x413 | **No** | — | — |
| `v_dot2_i32_i16` | 0x414 | **No** | — | — |
| `v_dot2_u32_u16` | 0x415 | **No** | — | — |
| `v_dot4_i32_i8` | 0x416 | **No** | — | — |
| `v_dot4_u32_u8` | 0x417 | **No** | — | — |
| `v_dot8_i32_i4` | 0x418 | **No** | — | — |
| `v_dot8_u32_u4` | 0x419 | **No** | — | — |
| `v_fma_mix_f32` | 0x420 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 19,544 |
| `v_fma_mixlo_f16` | 0x421 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8,396 |
| `v_fma_mixhi_f16` | 0x422 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,946 |

## Interpolation (VINTRP)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `v_interp_p1_f32` | 0x0 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 13,513 |
| `v_interp_p2_f32` | 0x1 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 13,513 |
| `v_interp_mov_f32` | 0x2 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 4,326 |

## Group memory (DS)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `ds_add_u32` | 0x0 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 260 |
| `ds_sub_u32` | 0x1 | **No** | — | — |
| `ds_rsub_u32` | 0x2 | **No** | — | — |
| `ds_inc_u32` | 0x3 | **No** | — | — |
| `ds_dec_u32` | 0x4 | **No** | — | — |
| `ds_min_i32` | 0x5 | **No** | — | — |
| `ds_max_i32` | 0x6 | **No** | — | — |
| `ds_min_u32` | 0x7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 128 |
| `ds_max_u32` | 0x8 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 78 |
| `ds_and_b32` | 0x9 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4 |
| `ds_or_b32` | 0xa | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 142 |
| `ds_xor_b32` | 0xb | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `ds_mskor_b32` | 0xc | **No** | — | — |
| `ds_write_b32` | 0xd | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 929 |
| `ds_write2_b32` | 0xe | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,018 |
| `ds_write2st64_b32` | 0xf | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 268 |
| `ds_cmpst_b32` | 0x10 | **No** | — | — |
| `ds_cmpst_f32` | 0x11 | **No** | — | — |
| `ds_min_f32` | 0x12 | **No** | — | — |
| `ds_max_f32` | 0x13 | **No** | — | — |
| `ds_nop` | 0x14 | **No** | — | — |
| `ds_add_f32` | 0x15 | **No** | — | — |
| `ds_gws_sema_release_all` | 0x18 | **No** | — | — |
| `ds_gws_init` | 0x19 | **No** | — | — |
| `ds_gws_sema_v` | 0x1a | **No** | — | — |
| `ds_gws_sema_br` | 0x1b | **No** | — | — |
| `ds_gws_sema_p` | 0x1c | **No** | — | — |
| `ds_gws_barrier` | 0x1d | **No** | — | — |
| `ds_write_b8` | 0x1e | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `ds_write_b16` | 0x1f | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 34 |
| `ds_add_rtn_u32` | 0x20 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 154 |
| `ds_sub_rtn_u32` | 0x21 | **No** | — | — |
| `ds_rsub_rtn_u32` | 0x22 | **No** | — | — |
| `ds_inc_rtn_u32` | 0x23 | **No** | — | — |
| `ds_dec_rtn_u32` | 0x24 | **No** | — | — |
| `ds_min_rtn_i32` | 0x25 | **No** | — | — |
| `ds_max_rtn_i32` | 0x26 | **No** | — | — |
| `ds_min_rtn_u32` | 0x27 | Yes | — | — |
| `ds_max_rtn_u32` | 0x28 | Yes | — | — |
| `ds_and_rtn_b32` | 0x29 | Yes | — | — |
| `ds_or_rtn_b32` | 0x2a | Yes | — | — |
| `ds_xor_rtn_b32` | 0x2b | Yes | — | — |
| `ds_mskor_rtn_b32` | 0x2c | **No** | — | — |
| `ds_wrxchg_rtn_b32` | 0x2d | Yes | — | — |
| `ds_wrxchg2_rtn_b32` | 0x2e | **No** | — | — |
| `ds_wrxchg2st64_rtn_b32` | 0x2f | **No** | — | — |
| `ds_cmpst_rtn_b32` | 0x30 | **No** | — | — |
| `ds_cmpst_rtn_f32` | 0x31 | **No** | — | — |
| `ds_min_rtn_f32` | 0x32 | **No** | — | — |
| `ds_max_rtn_f32` | 0x33 | **No** | — | — |
| `ds_wrap_rtn_b32` | 0x34 | **No** | — | — |
| `ds_swizzle_b32` | 0x35 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 8,507 |
| `ds_read_b32` | 0x36 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,119 |
| `ds_read2_b32` | 0x37 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 1,000 |
| `ds_read2st64_b32` | 0x38 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 180 |
| `ds_read_i8` | 0x39 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `ds_read_u8` | 0x3a | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `ds_read_i16` | 0x3b | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | — |
| `ds_read_u16` | 0x3c | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 32 |
| `ds_consume` | 0x3d | **No** | — | — |
| `ds_append` | 0x3e | **No** | — | — |
| `ds_ordered_count` | 0x3f | **No** | — | — |
| `ds_add_u64` | 0x40 | **No** | — | — |
| `ds_sub_u64` | 0x41 | **No** | — | — |
| `ds_rsub_u64` | 0x42 | **No** | — | — |
| `ds_inc_u64` | 0x43 | **No** | — | — |
| `ds_dec_u64` | 0x44 | **No** | — | — |
| `ds_min_i64` | 0x45 | **No** | — | — |
| `ds_max_i64` | 0x46 | **No** | — | — |
| `ds_min_u64` | 0x47 | **No** | — | — |
| `ds_max_u64` | 0x48 | **No** | — | — |
| `ds_and_b64` | 0x49 | **No** | — | — |
| `ds_or_b64` | 0x4a | **No** | — | — |
| `ds_xor_b64` | 0x4b | **No** | — | — |
| `ds_mskor_b64` | 0x4c | **No** | — | — |
| `ds_write_b64` | 0x4d | Yes | — | — |
| `ds_write2_b64` | 0x4e | Yes | — | — |
| `ds_write2st64_b64` | 0x4f | **No** | — | — |
| `ds_cmpst_b64` | 0x50 | **No** | — | — |
| `ds_cmpst_f64` | 0x51 | **No** | — | — |
| `ds_min_f64` | 0x52 | **No** | — | — |
| `ds_max_f64` | 0x53 | **No** | — | — |
| `ds_add_rtn_f32` | 0x55 | **No** | — | — |
| `ds_add_rtn_u64` | 0x60 | **No** | — | — |
| `ds_sub_rtn_u64` | 0x61 | **No** | — | — |
| `ds_rsub_rtn_u64` | 0x62 | **No** | — | — |
| `ds_inc_rtn_u64` | 0x63 | **No** | — | — |
| `ds_dec_rtn_u64` | 0x64 | **No** | — | — |
| `ds_min_rtn_i64` | 0x65 | **No** | — | — |
| `ds_max_rtn_i64` | 0x66 | **No** | — | — |
| `ds_min_rtn_u64` | 0x67 | **No** | — | — |
| `ds_max_rtn_u64` | 0x68 | **No** | — | — |
| `ds_and_rtn_b64` | 0x69 | **No** | — | — |
| `ds_or_rtn_b64` | 0x6a | **No** | — | — |
| `ds_xor_rtn_b64` | 0x6b | **No** | — | — |
| `ds_mskor_rtn_b64` | 0x6c | **No** | — | — |
| `ds_wrxchg_rtn_b64` | 0x6d | **No** | — | — |
| `ds_wrxchg2_rtn_b64` | 0x6e | **No** | — | — |
| `ds_wrxchg2st64_rtn_b64` | 0x6f | **No** | — | — |
| `ds_cmpst_rtn_b64` | 0x70 | **No** | — | — |
| `ds_cmpst_rtn_f64` | 0x71 | **No** | — | — |
| `ds_min_rtn_f64` | 0x72 | **No** | — | — |
| `ds_max_rtn_f64` | 0x73 | **No** | — | — |
| `ds_read_b64` | 0x76 | Yes | — | — |
| `ds_read2_b64` | 0x77 | Yes | — | — |
| `ds_read2st64_b64` | 0x78 | Yes | — | — |
| `ds_condxchg32_rtn_b64` | 0x7e | **No** | — | — |
| `ds_write_b8_d16_hi` | 0xa0 | **No** | — | — |
| `ds_write_b16_d16_hi` | 0xa1 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4 |
| `ds_read_u8_d16` | 0xa2 | **No** | — | — |
| `ds_read_u8_d16_hi` | 0xa3 | **No** | — | — |
| `ds_read_i8_d16` | 0xa4 | **No** | — | — |
| `ds_read_i8_d16_hi` | 0xa5 | **No** | — | — |
| `ds_read_u16_d16` | 0xa6 | Yes | — | — |
| `ds_read_u16_d16_hi` | 0xa7 | Yes | AMD 1002:13c0, NVIDIA 10de:2705 | 4 |
| `ds_write_addtid_b32` | 0xb0 | **No** | — | — |
| `ds_read_addtid_b32` | 0xb1 | **No** | — | — |
| `ds_permute_b32` | 0xb2 | **No** | — | — |
| `ds_bpermute_b32` | 0xb3 | Yes | — | — |
| `ds_write_b96` | 0xde | **No** | — | — |
| `ds_write_b128` | 0xdf | Yes | — | — |
| `ds_read_b96` | 0xfe | **No** | — | — |
| `ds_read_b128` | 0xff | **No** | — | — |

## Flat, global and scratch memory (FLAT)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `flat_load_ubyte` | 0x8 | **No** | — | — |
| `flat_load_sbyte` | 0x9 | **No** | — | — |
| `flat_load_ushort` | 0xa | Yes, in recorded shaders | — | 4 |
| `flat_load_sshort` | 0xb | **No** | — | — |
| `flat_load_dword` | 0xc | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 500 |
| `flat_load_dwordx2` | 0xd | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 537 |
| `flat_load_dwordx4` | 0xe | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 21 |
| `flat_load_dwordx3` | 0xf | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 26 |
| `flat_store_byte` | 0x18 | **No** | — | — |
| `flat_store_byte_d16_hi` | 0x19 | **No** | — | — |
| `flat_store_short` | 0x1a | **No** | — | — |
| `flat_store_short_d16_hi` | 0x1b | **No** | — | — |
| `flat_store_dword` | 0x1c | **No** | — | — |
| `flat_store_dwordx2` | 0x1d | **No** | — | — |
| `flat_store_dwordx4` | 0x1e | **No** | — | — |
| `flat_store_dwordx3` | 0x1f | **No** | — | — |
| `flat_load_ubyte_d16` | 0x20 | **No** | — | — |
| `flat_load_ubyte_d16_hi` | 0x21 | **No** | — | — |
| `flat_load_sbyte_d16` | 0x22 | **No** | — | — |
| `flat_load_sbyte_d16_hi` | 0x23 | **No** | — | — |
| `flat_load_short_d16` | 0x24 | **No** | — | — |
| `flat_load_short_d16_hi` | 0x25 | **No** | — | — |
| `flat_atomic_swap` | 0x30 | **No** | — | — |
| `flat_atomic_cmpswap` | 0x31 | **No** | — | — |
| `flat_atomic_add` | 0x32 | **No** | — | — |
| `flat_atomic_sub` | 0x33 | **No** | — | — |
| `flat_atomic_smin` | 0x35 | **No** | — | — |
| `flat_atomic_umin` | 0x36 | **No** | — | — |
| `flat_atomic_smax` | 0x37 | **No** | — | — |
| `flat_atomic_umax` | 0x38 | **No** | — | — |
| `flat_atomic_and` | 0x39 | **No** | — | — |
| `flat_atomic_or` | 0x3a | **No** | — | — |
| `flat_atomic_xor` | 0x3b | **No** | — | — |
| `flat_atomic_inc` | 0x3c | **No** | — | — |
| `flat_atomic_dec` | 0x3d | **No** | — | — |
| `flat_atomic_fcmpswap` | 0x3e | **No** | — | — |
| `flat_atomic_fmin` | 0x3f | **No** | — | — |
| `flat_atomic_fmax` | 0x40 | **No** | — | — |
| `flat_atomic_swap_x2` | 0x50 | **No** | — | — |
| `flat_atomic_cmpswap_x2` | 0x51 | **No** | — | — |
| `flat_atomic_add_x2` | 0x52 | **No** | — | — |
| `flat_atomic_sub_x2` | 0x53 | **No** | — | — |
| `flat_atomic_smin_x2` | 0x55 | **No** | — | — |
| `flat_atomic_umin_x2` | 0x56 | **No** | — | — |
| `flat_atomic_smax_x2` | 0x57 | **No** | — | — |
| `flat_atomic_umax_x2` | 0x58 | **No** | — | — |
| `flat_atomic_and_x2` | 0x59 | **No** | — | — |
| `flat_atomic_or_x2` | 0x5a | **No** | — | — |
| `flat_atomic_xor_x2` | 0x5b | **No** | — | — |
| `flat_atomic_inc_x2` | 0x5c | **No** | — | — |
| `flat_atomic_dec_x2` | 0x5d | **No** | — | — |
| `flat_atomic_fcmpswap_x2` | 0x5e | **No** | — | — |
| `flat_atomic_fmin_x2` | 0x5f | **No** | — | — |
| `flat_atomic_fmax_x2` | 0x60 | **No** | — | — |

## Untyped buffers (MUBUF)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `buffer_load_format_x` | 0x0 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 20,976 |
| `buffer_load_format_xy` | 0x1 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 4,272 |
| `buffer_load_format_xyz` | 0x2 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 6,430 |
| `buffer_load_format_xyzw` | 0x3 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 1,075 |
| `buffer_store_format_x` | 0x4 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_format_xy` | 0x5 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_format_xyz` | 0x6 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_format_xyzw` | 0x7 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 6,550 |
| `buffer_load_ubyte` | 0x8 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_sbyte` | 0x9 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_ushort` | 0xa | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_sshort` | 0xb | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_dword` | 0xc | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 31,035 |
| `buffer_load_dwordx2` | 0xd | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 25,203 |
| `buffer_load_dwordx4` | 0xe | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 23,179 |
| `buffer_load_dwordx3` | 0xf | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 13,984 |
| `buffer_store_byte` | 0x18 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_byte_d16_hi` | 0x19 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_short` | 0x1a | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_short_d16_hi` | 0x1b | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_dword` | 0x1c | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 2,496 |
| `buffer_store_dwordx2` | 0x1d | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 462 |
| `buffer_store_dwordx4` | 0x1e | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 1,048 |
| `buffer_store_dwordx3` | 0x1f | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 1,530 |
| `buffer_load_ubyte_d16` | 0x20 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_ubyte_d16_hi` | 0x21 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_sbyte_d16` | 0x22 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_sbyte_d16_hi` | 0x23 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_short_d16` | 0x24 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_short_d16_hi` | 0x25 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_format_d16_hi_x` | 0x26 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_format_d16_hi_x` | 0x27 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_swap` | 0x30 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 2 |
| `buffer_atomic_cmpswap` | 0x31 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 108 |
| `buffer_atomic_add` | 0x32 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 3,328 |
| `buffer_atomic_sub` | 0x33 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_csub` | 0x34 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_smin` | 0x35 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 26 |
| `buffer_atomic_umin` | 0x36 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 2,388 |
| `buffer_atomic_smax` | 0x37 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 34 |
| `buffer_atomic_umax` | 0x38 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 46 |
| `buffer_atomic_and` | 0x39 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 4 |
| `buffer_atomic_or` | 0x3a | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 74 |
| `buffer_atomic_xor` | 0x3b | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_inc` | 0x3c | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_dec` | 0x3d | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_fcmpswap` | 0x3e | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_fmin` | 0x3f | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_fmax` | 0x40 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_swap_x2` | 0x50 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_cmpswap_x2` | 0x51 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_add_x2` | 0x52 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_sub_x2` | 0x53 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_smin_x2` | 0x55 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_umin_x2` | 0x56 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_smax_x2` | 0x57 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_umax_x2` | 0x58 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_and_x2` | 0x59 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_or_x2` | 0x5a | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_xor_x2` | 0x5b | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_inc_x2` | 0x5c | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_dec_x2` | 0x5d | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_fcmpswap_x2` | 0x5e | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_fmin_x2` | 0x5f | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_atomic_fmax_x2` | 0x60 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_gl0_inv` | 0x71 | Yes | — | — |
| `buffer_gl1_inv` | 0x72 | Yes | — | — |
| `buffer_load_format_d16_x` | 0x80 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 72 |
| `buffer_load_format_d16_xy` | 0x81 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_format_d16_xyz` | 0x82 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_load_format_d16_xyzw` | 0x83 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 76 |
| `buffer_store_format_d16_x` | 0x84 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_format_d16_xy` | 0x85 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_format_d16_xyz` | 0x86 | Unproven: mubuf: s8 not a buffer descriptor (None) | — | — |
| `buffer_store_format_d16_xyzw` | 0x87 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 132 |

## Typed buffers (MTBUF)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `tbuffer_load_format_x` | 0x0 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 316 |
| `tbuffer_load_format_xy` | 0x1 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 18,506 |
| `tbuffer_load_format_xyz` | 0x2 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 5,231 |
| `tbuffer_load_format_xyzw` | 0x3 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 12,152 |
| `tbuffer_store_format_x` | 0x4 | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_store_format_xy` | 0x5 | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_store_format_xyz` | 0x6 | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_store_format_xyzw` | 0x7 | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_load_format_d16_x` | 0x8 | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_load_format_d16_xy` | 0x9 | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_load_format_d16_xyz` | 0xa | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_load_format_d16_xyzw` | 0xb | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_store_format_d16_x` | 0xc | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_store_format_d16_xy` | 0xd | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_store_format_d16_xyz` | 0xe | Unproven: typed buffer load requires a verified linear root SRV | — | — |
| `tbuffer_store_format_d16_xyzw` | 0xf | Unproven: typed buffer load requires a verified linear root SRV | — | — |

## Images (MIMG)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `image_load` | 0x0 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 18,145 |
| `image_load_mip` | 0x1 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 770 |
| `image_load_pck` | 0x2 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_pck_sgn` | 0x3 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_mip_pck` | 0x4 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_mip_pck_sgn` | 0x5 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store` | 0x8 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 9,914 |
| `image_store_mip` | 0x9 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_pck` | 0xa | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_mip_pck` | 0xb | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_get_resinfo` | 0xe | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 475 |
| `image_atomic_swap` | 0xf | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_cmpswap` | 0x10 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 4 |
| `image_atomic_add` | 0x11 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 25 |
| `image_atomic_sub` | 0x12 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_smin` | 0x14 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_umin` | 0x15 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 1,066 |
| `image_atomic_smax` | 0x16 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_umax` | 0x17 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 1,014 |
| `image_atomic_and` | 0x18 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_or` | 0x19 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 1,094 |
| `image_atomic_xor` | 0x1a | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_inc` | 0x1b | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_dec` | 0x1c | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_fcmpswap` | 0x1d | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_fmin` | 0x1e | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_atomic_fmax` | 0x1f | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample` | 0x20 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 15,441 |
| `image_sample_cl` | 0x21 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_d` | 0x22 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 3,149 |
| `image_sample_d_cl` | 0x23 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 48 |
| `image_sample_l` | 0x24 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 12,138 |
| `image_sample_b` | 0x25 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 1,034 |
| `image_sample_b_cl` | 0x26 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_lz` | 0x27 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 19,654 |
| `image_sample_c` | 0x28 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cl` | 0x29 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d` | 0x2a | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d_cl` | 0x2b | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_l` | 0x2c | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_b` | 0x2d | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_b_cl` | 0x2e | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_lz` | 0x2f | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 8,535 |
| `image_sample_o` | 0x30 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cl_o` | 0x31 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_d_o` | 0x32 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_d_cl_o` | 0x33 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_l_o` | 0x34 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_b_o` | 0x35 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_b_cl_o` | 0x36 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_lz_o` | 0x37 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_o` | 0x38 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cl_o` | 0x39 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d_o` | 0x3a | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d_cl_o` | 0x3b | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_l_o` | 0x3c | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_b_o` | 0x3d | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_b_cl_o` | 0x3e | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_lz_o` | 0x3f | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4` | 0x40 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_cl` | 0x41 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_by2` | 0x42 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_by4` | 0x43 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_l` | 0x44 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 88 |
| `image_gather4_b` | 0x45 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_b_cl` | 0x46 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_lz` | 0x47 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 8,973 |
| `image_gather4_c` | 0x48 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_cl` | 0x49 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_mip_by2` | 0x4a | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_mip_by4` | 0x4b | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_l` | 0x4c | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_b` | 0x4d | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_b_cl` | 0x4e | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_lz` | 0x4f | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_o` | 0x50 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_cl_o` | 0x51 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_by2` | 0x52 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_by4` | 0x53 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_l_o` | 0x54 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 88 |
| `image_gather4_b_o` | 0x55 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_b_cl_o` | 0x56 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_lz_o` | 0x57 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 12 |
| `image_gather4_c_o` | 0x58 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_cl_o` | 0x59 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_mip_by2` | 0x5a | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_mip_by4` | 0x5b | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_l_o` | 0x5c | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_b_o` | 0x5d | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_b_cl_o` | 0x5e | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4_c_lz_o` | 0x5f | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_get_lod` | 0x60 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 672 |
| `image_gather4h` | 0x61 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather4h_pck` | 0x62 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_gather8h_pck` | 0x63 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cd` | 0x68 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cd_cl` | 0x69 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd` | 0x6a | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd_cl` | 0x6b | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cd_o` | 0x6c | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cd_cl_o` | 0x6d | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd_o` | 0x6e | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd_cl_o` | 0x6f | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_pck2` | 0x70 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_pck4` | 0x71 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_mip_pck2` | 0x73 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_load_mip_pck4` | 0x74 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_pck2` | 0x76 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_pck4` | 0x77 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_mip_pck2` | 0x79 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_store_mip_pck4` | 0x7a | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_msaa_load` | 0x80 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_d_g16` | 0xa2 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_d_cl_g16` | 0xa3 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d_g16` | 0xaa | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d_cl_g16` | 0xab | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_d_o_g16` | 0xb2 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_d_cl_o_g16` | 0xb3 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d_o_g16` | 0xba | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_d_cl_o_g16` | 0xbb | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_bvh_intersect_ray` | 0xe6 | Unproven: raw BVH node intersection requires traversal translation | — | — |
| `image_bvh64_intersect_ray` | 0xe7 | Unproven: raw BVH node intersection requires traversal translation | — | — |
| `image_sample_cd_g16` | 0xe8 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cd_cl_g16` | 0xe9 | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd_g16` | 0xea | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd_cl_g16` | 0xeb | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cd_o_g16` | 0xec | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_cd_cl_o_g16` | 0xed | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd_o_g16` | 0xee | Unproven: mimg: s8 not an image descriptor (None) | — | — |
| `image_sample_c_cd_cl_o_g16` | 0xef | Unproven: mimg: s8 not an image descriptor (None) | — | — |

## Export (EXP)

| Instruction | Opcode | Translator | GPU check | Recorded shaders |
| --- | --- | --- | --- | ---: |
| `exp` | 0x0 | Yes, in recorded shaders | AMD 1002:13c0, NVIDIA 10de:2705 | 18,654 |
