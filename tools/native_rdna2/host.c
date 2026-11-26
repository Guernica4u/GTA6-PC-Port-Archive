// HSA host for kernels.hip: runs one instruction kernel over every input
// record for each MODE value given and writes the 64-bit results in order.
// It uses the HSA runtime directly; some packaged HIP runtimes fault on
// misaligned AVX-512 stores while loading code objects.
//   host kernels.co inputs.bin outputs.bin v_name mode...
#include <hsa/hsa.h>
#include <hsa/hsa_ext_amd.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { uint64_t a, b; uint32_t c, f; } In;

#define CK(e) do { hsa_status_t s_ = (e); if (s_ != HSA_STATUS_SUCCESS) { const char* m_; \
    hsa_status_string(s_, &m_); fprintf(stderr, "%s: %s\n", #e, m_); exit(2); } } while (0)

static hsa_agent_t gpu;
static int have_gpu, have_cpu, have_fine, have_kernarg;
static hsa_amd_memory_pool_t fine_pool, kernarg_pool;
static char product[64];

static hsa_status_t pools(hsa_amd_memory_pool_t p, void* unused) {
    hsa_amd_segment_t segment;
    uint32_t flags;
    (void)unused;
    hsa_amd_memory_pool_get_info(p, HSA_AMD_MEMORY_POOL_INFO_SEGMENT, &segment);
    if (segment != HSA_AMD_SEGMENT_GLOBAL) return HSA_STATUS_SUCCESS;
    hsa_amd_memory_pool_get_info(p, HSA_AMD_MEMORY_POOL_INFO_GLOBAL_FLAGS, &flags);
    if ((flags & HSA_AMD_MEMORY_POOL_GLOBAL_FLAG_KERNARG_INIT) && !have_kernarg) { kernarg_pool = p; have_kernarg = 1; }
    else if ((flags & HSA_AMD_MEMORY_POOL_GLOBAL_FLAG_FINE_GRAINED) && !have_fine) { fine_pool = p; have_fine = 1; }
    return HSA_STATUS_SUCCESS;
}

static hsa_status_t agents(hsa_agent_t a, void* unused) {
    hsa_device_type_t type;
    char name[64];
    (void)unused;
    hsa_agent_get_info(a, HSA_AGENT_INFO_DEVICE, &type);
    hsa_agent_get_info(a, HSA_AGENT_INFO_NAME, name);
    if (type == HSA_DEVICE_TYPE_GPU && !strcmp(name, "gfx1030") && !have_gpu) {
        gpu = a;
        have_gpu = 1;
        hsa_agent_get_info(a, (hsa_agent_info_t)HSA_AMD_AGENT_INFO_PRODUCT_NAME, product);
    }
    if (type == HSA_DEVICE_TYPE_CPU && !have_cpu) {
        have_cpu = 1;
        hsa_amd_agent_iterate_memory_pools(a, pools, NULL);
    }
    return HSA_STATUS_SUCCESS;
}

static void* shared(hsa_amd_memory_pool_t pool, size_t n) {
    void* p;
    CK(hsa_amd_memory_pool_allocate(pool, n, 0, &p));
    CK(hsa_amd_agents_allow_access(1, &gpu, NULL, p));
    return p;
}

static void* slurp(const char* path, size_t* n) {
    FILE* f = fopen(path, "rb");
    if (!f) { perror(path); exit(1); }
    fseek(f, 0, SEEK_END);
    *n = (size_t)ftell(f);
    rewind(f);
    void* p = malloc(*n);
    if (fread(p, 1, *n, f) != *n) exit(1);
    fclose(f);
    return p;
}

int main(int argc, char** argv) {
    if (argc < 6) { fprintf(stderr, "usage: host kernels.co inputs.bin outputs.bin v_name mode...\n"); return 1; }
    CK(hsa_init());
    hsa_iterate_agents(agents, NULL);
    if (!have_gpu || !have_fine || !have_kernarg) { fprintf(stderr, "no gfx1030 agent with host pools\n"); return 3; }
    printf("%s\n", product);

    size_t code_size, input_size;
    void* code = slurp(argv[1], &code_size);
    hsa_code_object_reader_t reader;
    hsa_executable_t exe;
    CK(hsa_code_object_reader_create_from_memory(code, code_size, &reader));
    CK(hsa_executable_create_alt(HSA_PROFILE_FULL, HSA_DEFAULT_FLOAT_ROUNDING_MODE_DEFAULT, NULL, &exe));
    CK(hsa_executable_load_agent_code_object(exe, gpu, reader, NULL, NULL));
    CK(hsa_executable_freeze(exe, NULL));
    char symbol_name[128];
    snprintf(symbol_name, sizeof symbol_name, "k_%s.kd", argv[4]);
    hsa_executable_symbol_t symbol;
    CK(hsa_executable_get_symbol_by_name(exe, symbol_name, &gpu, &symbol));
    uint64_t object;
    uint32_t kernarg_size, group_size, private_size;
    hsa_executable_symbol_get_info(symbol, HSA_EXECUTABLE_SYMBOL_INFO_KERNEL_OBJECT, &object);
    hsa_executable_symbol_get_info(symbol, HSA_EXECUTABLE_SYMBOL_INFO_KERNEL_KERNARG_SEGMENT_SIZE, &kernarg_size);
    hsa_executable_symbol_get_info(symbol, HSA_EXECUTABLE_SYMBOL_INFO_KERNEL_GROUP_SEGMENT_SIZE, &group_size);
    hsa_executable_symbol_get_info(symbol, HSA_EXECUTABLE_SYMBOL_INFO_KERNEL_PRIVATE_SEGMENT_SIZE, &private_size);

    In* records = slurp(argv[2], &input_size);
    size_t n = input_size / sizeof(In);
    if (n == 0 || n % 64) { fprintf(stderr, "input records must be a nonzero multiple of 64\n"); return 1; }
    In* in = shared(fine_pool, n * sizeof(In));
    uint64_t* out = shared(fine_pool, n * 8);
    memcpy(in, records, n * sizeof(In));
    void* kernarg = shared(kernarg_pool, kernarg_size < 64 ? 64 : kernarg_size);
    hsa_queue_t* queue;
    hsa_signal_t done;
    CK(hsa_queue_create(gpu, 64, HSA_QUEUE_TYPE_SINGLE, NULL, NULL, UINT32_MAX, UINT32_MAX, &queue));
    CK(hsa_signal_create(1, 0, NULL, &done));

    FILE* results = fopen(argv[3], "wb");
    for (int i = 5; i < argc; i++) {
        struct { const In* in; uint64_t* out; uint32_t mode; } args = {in, out, (uint32_t)strtoul(argv[i], NULL, 0)};
        memset(kernarg, 0, kernarg_size);
        memcpy(kernarg, &args, sizeof args);
        memset(out, 0xcd, n * 8);
        hsa_signal_store_relaxed(done, 1);
        uint64_t index = hsa_queue_add_write_index_relaxed(queue, 1);
        hsa_kernel_dispatch_packet_t* p = (hsa_kernel_dispatch_packet_t*)queue->base_address + (index & (queue->size - 1));
        memset((uint8_t*)p + 4, 0, sizeof *p - 4);
        p->setup = 1 << HSA_KERNEL_DISPATCH_PACKET_SETUP_DIMENSIONS;
        p->workgroup_size_x = 64;
        p->workgroup_size_y = p->workgroup_size_z = 1;
        p->grid_size_x = (uint32_t)n;
        p->grid_size_y = p->grid_size_z = 1;
        p->kernel_object = object;
        p->kernarg_address = kernarg;
        p->group_segment_size = group_size;
        p->private_segment_size = private_size;
        p->completion_signal = done;
        uint16_t header = HSA_PACKET_TYPE_KERNEL_DISPATCH << HSA_PACKET_HEADER_TYPE | 1 << HSA_PACKET_HEADER_BARRIER
            | HSA_FENCE_SCOPE_SYSTEM << HSA_PACKET_HEADER_SCACQUIRE_FENCE_SCOPE
            | HSA_FENCE_SCOPE_SYSTEM << HSA_PACKET_HEADER_SCRELEASE_FENCE_SCOPE;
        __atomic_store_n((uint32_t*)p, header | (uint32_t)p->setup << 16, __ATOMIC_RELEASE);
        hsa_signal_store_screlease(queue->doorbell_signal, (hsa_signal_value_t)index);
        while (hsa_signal_wait_scacquire(done, HSA_SIGNAL_CONDITION_LT, 1, UINT64_MAX, HSA_WAIT_STATE_BLOCKED) != 0) {}
        fwrite(out, 8, n, results);
    }
    fclose(results);
    return 0;
}
