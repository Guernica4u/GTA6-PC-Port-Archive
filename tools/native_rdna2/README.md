# Native RDNA2 reference checks

These files run single RDNA2 instructions natively on a gfx1030 GPU (RX 6800/6900/6950 series), so a reference model can be compared with the hardware itself instead of only with the ISA documentation or LLVM.

`kernels.hip` has one kernel per instruction. Each lane reads a synthetic input record, writes MODE[9:0] with `s_setreg_b32`, executes the instruction and stores the result. `host.c` loads the code object through the HSA runtime and runs a kernel once for each MODE value. It does not use the HIP runtime, because some packaged HIP runtimes fault while loading code objects.

`check_model.py` runs every instruction under all 1024 MODE values and works out which MODE bits change any result. It then compares the reference model of `tools/check_f64_modes_gpu.py` on every combination of those bits:

```
python tools/native_rdna2/check_model.py --work build/native-rdna2 --output build/native-rdna2.json
```

It needs ROCm (`hipcc`, `clang-offload-bundler` and `libhsa-runtime64`), by default under `/opt/rocm`. [Example receipt](../../evidence/native-rdna2-f64-modes.json)
