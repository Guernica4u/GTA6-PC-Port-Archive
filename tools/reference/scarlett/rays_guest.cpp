// Replays ProjectScarlett's irradiance-cache shadow programs through the Xbox
// provider. The translator replaces each inlined console BVH walk with a
// native ray query; this guest builds native scenes, fills the program's
// inputs and compares every lane's result with the geometry.
//
// Inputs, as the guest program reads them (CS_IrradianceCache_ShadowTrace*):
// - root constant 0: list index. t379[index] is the number of cache texels.
// - t339 (table 1, slot 0): texel words, cascade << 29 | texel.
// - t12 (table 0, slot 20): byte 1420 holds the grid size (b and c axes, then
//   a axis); byte 1428 + 16 * cascade the cascade centre xyz and half extent,
//   byte 1520 + 16 * cascade the half extent of the a axis, and byte 1992 the
//   near radius / 0.7 (WithFar only).
// - t10 (table 1, slot 1): byte 272 holds L. Shadow rays run along -L.
// - b3 (table 1, slot 7): byte 20 is the output stride per cascade, byte 68
//   the maximum distance.
// - t355 (slot 4) is the near scene, t356 (slot 3) the far scene.
// - u52 (slot 6): each texel gets 0.5 when unoccluded and 0 when occluded.
// Texel i with a 4x4 grid sits at x = -0.75 + 0.5 * (i / 16), y = -0.75 +
// 0.5 * ((i / 4) % 4) and z = -0.75 + 0.5 * (i % 4).
//
// CS_LaserLightShadowRaytracing (mode "laser") is recorded without a root
// signature; the guest declares two tables naming the program's registers
// (t47; t371, t354, u48 in space 1). Lamp i = 32 * group + thread:
// - table 1, slot 0: typed words; bit i % 32 of word i / 32 selects the lamp.
// - table 0, slot 5: 512-byte lamp records. Words 0-2 hold the beam origin,
//   the high half of word 3 its length (f16), words 5 and 6 its direction as
//   f16 x, y, z (normalised by the program).
// - table 1, slot 1: the scene.
// - table 1, slot 2: 8-byte records; word 0 gets the distance to the nearest
//   triangle plus 0.04, or FLT_MAX after a miss and for unselected lamps.
#include "frame_abi.h"
#include <cmath>
#include <cstring>
#include <fstream>
#include <iterator>
#include <vector>

using frame_abi::check;
using frame_abi::ComPtr;
using frame_abi::method;
using Inputs = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS;
using Sizes = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO;
using Build = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC;

static ComPtr<ID3D12Resource> buffer(ID3D12Device* dev, UINT64 bytes, D3D12_HEAP_TYPE heap,
                                     D3D12_RESOURCE_STATES state, bool uav = false) {
    D3D12_HEAP_PROPERTIES props{};
    props.Type = heap;
    props.CreationNodeMask = props.VisibleNodeMask = 1;
    D3D12_RESOURCE_DESC desc{};
    desc.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    desc.Width = bytes;
    desc.Height = desc.DepthOrArraySize = desc.MipLevels = desc.SampleDesc.Count = 1;
    desc.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    if (uav) desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    ComPtr<ID3D12Resource> res;
    check(dev->CreateCommittedResource(&props, D3D12_HEAP_FLAG_NONE, &desc, state, nullptr, IID_PPV_ARGS(&res)),
          "create buffer");
    return res;
}

static ComPtr<ID3D12Resource> upload(ID3D12Device* dev, const void* data, size_t bytes) {
    auto res = buffer(dev, bytes, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
    D3D12_RANGE empty{}, written{0, bytes};
    void* dest{};
    check(res->Map(0, &empty, &dest), "map upload");
    std::memcpy(dest, data, bytes);
    res->Unmap(0, &written);
    return res;
}

static ComPtr<ID3D12GraphicsCommandList> guest_list(ID3D12Device* guest, ID3D12CommandAllocator* allocator) {
    const UINT params[]{0, 0, 1, 0};
    ComPtr<ID3D12GraphicsCommandList> list;
    check(method<frame_abi::CreateList>(guest, 73)(guest, params, allocator, nullptr, IID_PPV_ARGS(&list)),
          "create Xbox list");
    return list;
}

static void submit(ID3D12Device* dev, ID3D12CommandQueue* queue, ID3D12GraphicsCommandList* list) {
    check(list->Close(), "close list");
    ID3D12CommandList* lists[] = {list};
    ComPtr<ID3D12Fence> fence;
    check(dev->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&fence)), "create fence");
    queue->ExecuteCommandLists(1, lists);
    check(queue->Signal(fence.Get(), 1), "signal fence");
    HANDLE event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!event) throw std::runtime_error("create event");
    HRESULT hr = fence->SetEventOnCompletion(1, event);
    DWORD wait = SUCCEEDED(hr) ? WaitForSingleObject(event, 30000) : WAIT_FAILED;
    CloseHandle(event);
    check(hr, "fence event");
    if (wait != WAIT_OBJECT_0 || fence->GetCompletedValue() != 1) throw std::runtime_error("GPU fence did not complete");
}

static std::vector<char> read_file(const wchar_t* path) {
    std::ifstream in(path, std::ios::binary);
    if (!in) throw std::runtime_error("cannot open shader");
    return {std::istreambuf_iterator<char>{in}, std::istreambuf_iterator<char>{}};
}

// An opaque axis-aligned rectangle at height z facing the rays.
struct Quad {
    float z, x0, x1, y0, y1;
};

struct Scene {
    ComPtr<ID3D12Resource> bottom, top, vertices, records;
};

struct Fixture {
    ComPtr<ID3D12Device> dev;
    ComPtr<ID3D12CommandQueue> queue;
    ComPtr<ID3D12RootSignature> root;
    ComPtr<ID3D12PipelineState> pipeline;

    // The title root signature embedded in the program, or one the guest
    // declares for a program recorded without one.
    explicit Fixture(const wchar_t* shader, const D3D12_ROOT_SIGNATURE_DESC* own = nullptr) {
        HMODULE module = LoadLibraryExW(L"d3d12_xs.dll", nullptr,
                                        LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
        using Factory = HRESULT(WINAPI*)(IUnknown*, const void*, REFIID, void**);
        auto factory = module ? reinterpret_cast<Factory>(GetProcAddress(module, "D3D12XboxCreateDevice")) : nullptr;
        if (!factory) throw std::runtime_error("missing Xbox device factory");
        const std::array<UINT, 18> profile{0x20004, 0, ~0u, 0, 0, 0, ~0u, ~0u, 0, 1};
        check(factory(nullptr, profile.data(), IID_PPV_ARGS(&dev)), "create Xbox device");
        D3D12_COMMAND_QUEUE_DESC q{};
        check(dev->CreateCommandQueue(&q, IID_PPV_ARGS(&queue)), "Xbox queue");
        const auto bytes = read_file(shader);
        if (own) {
            ComPtr<ID3DBlob> serialized;
            check(D3D12SerializeRootSignature(own, D3D_ROOT_SIGNATURE_VERSION_1, &serialized, nullptr),
                  "serialize tables");
            check(dev->CreateRootSignature(0, serialized->GetBufferPointer(), serialized->GetBufferSize(),
                                           IID_PPV_ARGS(&root)),
                  "table root");
        } else {
            check(dev->CreateRootSignature(0, bytes.data(), bytes.size(), IID_PPV_ARGS(&root)), "title root");
        }
        D3D12_COMPUTE_PIPELINE_STATE_DESC pso{};
        pso.pRootSignature = root.Get();
        pso.CS = {bytes.data(), bytes.size()};
        using Create = HRESULT(WINAPI*)(ID3D12Device*, const D3D12_COMPUTE_PIPELINE_STATE_DESC*, UINT, const void*,
                                        REFIID, void**);
        check(method<Create>(dev.Get(), 62)(dev.Get(), &pso, 0, nullptr, IID_PPV_ARGS(&pipeline)), "title pipeline");
    }

    ComPtr<ID3D12Resource> build(const Inputs& in) {
        Sizes sizes{};
        using Prebuild = void(WINAPI*)(ID3D12Device*, const Inputs*, Sizes*);
        method<Prebuild>(dev.Get(), 102)(dev.Get(), &in, &sizes);
        auto dest = buffer(dev.Get(), sizes.ResultDataMaxSizeInBytes, D3D12_HEAP_TYPE_DEFAULT,
                           D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE, true);
        auto scratch = buffer(dev.Get(), sizes.ScratchDataSizeInBytes ? sizes.ScratchDataSizeInBytes : 256,
                              D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, true);
        ComPtr<ID3D12CommandAllocator> alloc;
        check(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc)), "AS allocator");
        auto list = guest_list(dev.Get(), alloc.Get());
        ComPtr<ID3D12GraphicsCommandList4> list4;
        check(list.As(&list4), "Xbox List4");
        Build b{};
        b.Inputs = in;
        b.DestAccelerationStructureData = dest->GetGPUVirtualAddress();
        b.ScratchAccelerationStructureData = scratch->GetGPUVirtualAddress();
        using Single = void(WINAPI*)(ID3D12GraphicsCommandList4*, const Build*, UINT,
                                     const D3D12_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO_DESC*);
        method<Single>(list4.Get(), 117)(list4.Get(), &b, 0, nullptr);
        D3D12_RESOURCE_BARRIER barrier{};
        barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_UAV;
        list->ResourceBarrier(1, &barrier);
        submit(dev.Get(), queue.Get(), list.Get());
        return dest;
    }

    // One BLAS holding every quad, one instance of it; no instance at all for
    // an empty scene.
    Scene scene(const std::vector<Quad>& quads) {
        Scene s;
        std::vector<float> v;
        for (const auto& q : quads) {
            const float corners[6][3] = {{q.x0, q.y0, q.z}, {q.x1, q.y0, q.z}, {q.x1, q.y1, q.z},
                                         {q.x0, q.y0, q.z}, {q.x1, q.y1, q.z}, {q.x0, q.y1, q.z}};
            for (const auto& c : corners) v.insert(v.end(), c, c + 3);
        }
        Inputs top{};
        top.Type = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL;
        if (!quads.empty()) {
            s.vertices = upload(dev.Get(), v.data(), v.size() * sizeof(float));
            D3D12_RAYTRACING_GEOMETRY_DESC geom{};
            geom.Type = D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES;
            geom.Flags = D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE;
            geom.Triangles.VertexFormat = DXGI_FORMAT_R32G32B32_FLOAT;
            geom.Triangles.VertexCount = UINT(v.size() / 3);
            geom.Triangles.VertexBuffer = {s.vertices->GetGPUVirtualAddress(), 3 * sizeof(float)};
            Inputs bottom{};
            bottom.Type = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL;
            bottom.NumDescs = 1;
            bottom.pGeometryDescs = &geom;
            s.bottom = build(bottom);
            D3D12_RAYTRACING_INSTANCE_DESC inst{};
            for (UINT axis = 0; axis < 3; ++axis) inst.Transform[axis][axis] = 1.f;
            inst.InstanceMask = 0xff;
            inst.AccelerationStructure = s.bottom->GetGPUVirtualAddress();
            s.records = upload(dev.Get(), &inst, sizeof(inst));
            top.NumDescs = 1;
            top.InstanceDescs = s.records->GetGPUVirtualAddress();
        }
        s.top = build(top);
        return s;
    }
};

struct Scenario {
    const char* name;
    float tmax, radius;
    std::vector<Quad> near_quads, far_quads;
};

constexpr UINT lanes = 32;

static void origin(UINT i, float p[3]) {
    p[0] = -0.75f + 0.5f * float(i / 16);
    p[1] = -0.75f + 0.5f * float((i / 4) % 4);
    p[2] = -0.75f + 0.5f * float(i % 4);
}

// Rays run along +z. A quad is hit at t = z - origin.z when the origin lies
// inside it and minimum <= t < maximum.
static bool hits(const std::vector<Quad>& quads, const float p[3], float start, float maximum) {
    for (const auto& q : quads) {
        const float t = q.z - (p[2] + start);
        if (t >= 0 && t < maximum && p[0] > q.x0 && p[0] < q.x1 && p[1] > q.y0 && p[1] < q.y1) return true;
    }
    return false;
}

// What the guest program computes after its walks. The far walk runs only
// when the near walk missed and the ray still has length after leaving the
// near sphere; it starts where the ray leaves that sphere.
static bool occluded(const Scenario& s, UINT i, bool with_far) {
    float p[3];
    origin(i, p);
    if (hits(s.near_quads, p, 0, s.tmax)) return true;
    if (!with_far) return false;
    const float length2 = p[0] * p[0] + p[1] * p[1] + p[2] * p[2];
    const float r2 = (0.7f * s.radius) * (0.7f * s.radius);
    const float exit = length2 < r2 ? std::fmax(0.f, std::sqrt(r2 - length2 + p[2] * p[2]) - p[2]) : 0.f;
    return s.tmax - exit > 0 && hits(s.far_quads, p, exit, s.tmax - exit);
}

static UINT bits(float x) {
    UINT u;
    std::memcpy(&u, &x, 4);
    return u;
}

static bool run(Fixture& f, const Scenario& s, bool with_far, FILE* receipt, const char* label) {
    auto dev = f.dev.Get();
    const auto near_scene = f.scene(s.near_quads);
    const auto far_scene = f.scene(s.far_quads);
    D3D12_DESCRIPTOR_HEAP_DESC hd{D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 128, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, 0};
    ComPtr<ID3D12DescriptorHeap> heap;
    check(dev->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&heap)), "view descriptors");
    const auto step = dev->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    using Gpu = UINT64(WINAPI*)(ID3D12DescriptorHeap*);
    auto cpu = [&](UINT i) {
        return D3D12_CPU_DESCRIPTOR_HANDLE{method<frame_abi::CpuHandle>(heap.Get(), 9)(heap.Get()) + i * step};
    };
    auto gpu = [&](UINT i) { return D3D12_GPU_DESCRIPTOR_HANDLE{method<Gpu>(heap.Get(), 10)(heap.Get()) + i * step}; };
    auto typed = [&](UINT slot, ID3D12Resource* res, UINT count) {
        D3D12_SHADER_RESOURCE_VIEW_DESC view{};
        view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        view.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
        view.Format = DXGI_FORMAT_R32_UINT;
        view.Buffer.NumElements = count;
        dev->CreateShaderResourceView(res, &view, cpu(slot));
    };
    auto structured = [&](UINT slot, ID3D12Resource* res, UINT stride) {
        D3D12_SHADER_RESOURCE_VIEW_DESC view{};
        view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        view.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
        view.Buffer.StructureByteStride = stride;
        view.Buffer.NumElements = 1;
        dev->CreateShaderResourceView(res, &view, cpu(slot));
    };
    auto scene_view = [&](UINT slot, ID3D12Resource* res) {
        D3D12_SHADER_RESOURCE_VIEW_DESC view{};
        view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        view.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
        view.Format = DXGI_FORMAT_R32_TYPELESS;
        view.Buffer.Flags = D3D12_BUFFER_SRV_FLAG_RAW;
        view.Buffer.NumElements = 32;
        dev->CreateShaderResourceView(res, &view, cpu(slot));
    };
    // Table 0 starts at 0, table 1 at 64.
    std::vector<UINT> grid(511);
    grid[1420 / 4] = 4;
    grid[1424 / 4] = 4;
    grid[1428 / 4 + 3] = bits(1.f);
    grid[1520 / 4] = bits(1.f);
    grid[1992 / 4] = bits(s.radius);
    auto grid_buffer = upload(dev, grid.data(), grid.size() * 4);
    structured(20, grid_buffer.Get(), 2044);
    std::vector<UINT> texels(lanes);
    for (UINT i = 0; i < lanes; ++i) texels[i] = i;
    auto texel_buffer = upload(dev, texels.data(), texels.size() * 4);
    typed(64, texel_buffer.Get(), lanes);
    std::vector<UINT> light(85);
    light[272 / 4 + 2] = bits(-1.f);
    auto light_buffer = upload(dev, light.data(), light.size() * 4);
    structured(65, light_buffer.Get(), 340);
    const UINT count[4]{lanes, 0, 0, 0};
    auto count_buffer = upload(dev, count, sizeof(count));
    typed(66, count_buffer.Get(), 4);
    scene_view(67, far_scene.top.Get());
    scene_view(68, near_scene.top.Get());
    typed(69, count_buffer.Get(), 4);
    std::vector<float> seed(4 * 64, 7.f);
    auto out = buffer(dev, seed.size() * 4, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COPY_DEST, true);
    auto read = buffer(dev, seed.size() * 4, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
    auto seed_buffer = upload(dev, seed.data(), seed.size() * 4);
    D3D12_UNORDERED_ACCESS_VIEW_DESC uav{};
    uav.ViewDimension = D3D12_UAV_DIMENSION_BUFFER;
    uav.Format = DXGI_FORMAT_R32G32B32A32_FLOAT;
    uav.Buffer.NumElements = 64;
    dev->CreateUnorderedAccessView(out.Get(), nullptr, &uav, cpu(70));
    std::vector<UINT> constants(64);
    constants[5] = lanes;
    constants[17] = bits(s.tmax);
    auto cb = upload(dev, constants.data(), constants.size() * 4);
    D3D12_CONSTANT_BUFFER_VIEW_DESC cbv{cb->GetGPUVirtualAddress(), 256};
    dev->CreateConstantBufferView(&cbv, cpu(71));

    ComPtr<ID3D12CommandAllocator> alloc;
    check(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc)), "allocator");
    auto list = guest_list(dev, alloc.Get());
    list->CopyBufferRegion(out.Get(), 0, seed_buffer.Get(), 0, seed.size() * 4);
    D3D12_RESOURCE_BARRIER barrier{};
    barrier.Transition = {out.Get(), 0, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS};
    list->ResourceBarrier(1, &barrier);
    list->SetPipelineState(f.pipeline.Get());
    ID3D12DescriptorHeap* heaps[]{heap.Get()};
    using Heaps = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, ID3D12DescriptorHeap* const*);
    using Root = void(WINAPI*)(ID3D12GraphicsCommandList*, ID3D12RootSignature*);
    using Table = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, D3D12_GPU_DESCRIPTOR_HANDLE);
    using Constants = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, UINT, const void*, UINT);
    method<Heaps>(list.Get(), 32)(list.Get(), 1, heaps);
    method<Root>(list.Get(), 33)(list.Get(), f.root.Get());
    method<Table>(list.Get(), 35)(list.Get(), 0, gpu(0));
    method<Table>(list.Get(), 35)(list.Get(), 1, gpu(64));
    const UINT index[4]{0, 0, 0, 0};
    method<Constants>(list.Get(), 39)(list.Get(), 2, 4, index, 0);
    list->Dispatch(1, 1, 1);
    barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    list->ResourceBarrier(1, &barrier);
    list->CopyBufferRegion(read.Get(), 0, out.Get(), 0, seed.size() * 4);
    submit(dev, f.queue.Get(), list.Get());
    D3D12_RANGE range{0, seed.size() * 4}, none{};
    void* data{};
    check(read->Map(0, &range, &data), "map result");
    const float* got = static_cast<const float*>(data);
    UINT expected_mask = 0, observed_mask = 0, bad = 0;
    for (UINT i = 0; i < lanes; ++i) {
        const bool hit = occluded(s, i, with_far);
        const float want = hit ? 0.f : 0.5f;
        expected_mask |= UINT(hit) << i;
        observed_mask |= UINT(got[4 * i] == 0.f) << i;
        for (UINT c = 0; c < 4; ++c) bad += got[4 * i + c] != want;
    }
    for (UINT i = lanes; i < 64; ++i)
        for (UINT c = 0; c < 4; ++c) bad += got[4 * i + c] != 7.f;
    read->Unmap(0, &none);
    std::fprintf(receipt, "%s_%s=expected %08x observed %08x mismatches %u\n", label, s.name, expected_mask,
                 observed_mask, bad);
    std::fflush(receipt);
    return bad == 0;
}

// A lamp beam: origin, direction (not normalised), length; selected or not.
struct Beam {
    float p[3], d[3], length;
    bool selected;
};

struct LaserScenario {
    const char* name;
    std::vector<Quad> quads;
    std::vector<Beam> beams;
};

static UINT half(float x) {
    // Exact for the values used here: small integers and halves.
    if (x == 0.f) return 0;
    UINT u = bits(x), sign = (u >> 16) & 0x8000u;
    int e = int((u >> 23) & 0xff) - 127 + 15;
    return sign | UINT(e) << 10 | ((u >> 13) & 0x3ffu);
}

// The nearest quad along the beam with 0 <= t < length; quads face z.
static float nearest(const std::vector<Quad>& quads, const Beam& b) {
    const float n = std::sqrt(b.d[0] * b.d[0] + b.d[1] * b.d[1] + b.d[2] * b.d[2]);
    float best = INFINITY;
    for (const auto& q : quads) {
        if (b.d[2] == 0.f) continue;
        const float t = (q.z - b.p[2]) / (b.d[2] / n);
        const float x = b.p[0] + t * b.d[0] / n, y = b.p[1] + t * b.d[1] / n;
        if (t >= 0 && t < b.length && x > q.x0 && x < q.x1 && y > q.y0 && y < q.y1) best = std::fmin(best, t);
    }
    return best;
}

static bool run_laser(Fixture& f, const LaserScenario& s, FILE* receipt) {
    auto dev = f.dev.Get();
    const auto scene = f.scene(s.quads);
    D3D12_DESCRIPTOR_HEAP_DESC hd{D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 128, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, 0};
    ComPtr<ID3D12DescriptorHeap> heap;
    check(dev->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&heap)), "view descriptors");
    const auto step = dev->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    using Gpu = UINT64(WINAPI*)(ID3D12DescriptorHeap*);
    auto cpu = [&](UINT i) {
        return D3D12_CPU_DESCRIPTOR_HANDLE{method<frame_abi::CpuHandle>(heap.Get(), 9)(heap.Get()) + i * step};
    };
    auto gpu = [&](UINT i) { return D3D12_GPU_DESCRIPTOR_HANDLE{method<Gpu>(heap.Get(), 10)(heap.Get()) + i * step}; };
    const UINT count = UINT(s.beams.size());
    std::vector<UINT> records(128 * lanes), mask(1);
    for (UINT i = 0; i < count; ++i) {
        const auto& b = s.beams[i];
        UINT* r = &records[128 * i];
        for (UINT c = 0; c < 3; ++c) r[c] = bits(b.p[c]);
        r[3] = half(b.length) << 16;
        r[5] = half(b.d[0]) | half(b.d[1]) << 16;
        r[6] = half(b.d[2]);
        mask[0] |= UINT(b.selected) << i;
    }
    auto record_buffer = upload(dev, records.data(), records.size() * 4);
    D3D12_SHADER_RESOURCE_VIEW_DESC view{};
    view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    view.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
    view.Buffer.StructureByteStride = 512;
    view.Buffer.NumElements = lanes;
    dev->CreateShaderResourceView(record_buffer.Get(), &view, cpu(5));
    auto mask_buffer = upload(dev, mask.data(), mask.size() * 4);
    view.Buffer.StructureByteStride = 0;
    view.Format = DXGI_FORMAT_R32_UINT;
    view.Buffer.NumElements = 1;
    dev->CreateShaderResourceView(mask_buffer.Get(), &view, cpu(64));
    view.Format = DXGI_FORMAT_R32_TYPELESS;
    view.Buffer.Flags = D3D12_BUFFER_SRV_FLAG_RAW;
    view.Buffer.NumElements = 32;
    dev->CreateShaderResourceView(scene.top.Get(), &view, cpu(65));
    std::vector<UINT> seed(2 * lanes, 7);
    auto out = buffer(dev, seed.size() * 4, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COPY_DEST, true);
    auto read = buffer(dev, seed.size() * 4, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
    auto seed_buffer = upload(dev, seed.data(), seed.size() * 4);
    D3D12_UNORDERED_ACCESS_VIEW_DESC uav{};
    uav.ViewDimension = D3D12_UAV_DIMENSION_BUFFER;
    uav.Buffer.StructureByteStride = 8;
    uav.Buffer.NumElements = lanes;
    dev->CreateUnorderedAccessView(out.Get(), nullptr, &uav, cpu(66));

    ComPtr<ID3D12CommandAllocator> alloc;
    check(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc)), "allocator");
    auto list = guest_list(dev, alloc.Get());
    list->CopyBufferRegion(out.Get(), 0, seed_buffer.Get(), 0, seed.size() * 4);
    D3D12_RESOURCE_BARRIER barrier{};
    barrier.Transition = {out.Get(), 0, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS};
    list->ResourceBarrier(1, &barrier);
    list->SetPipelineState(f.pipeline.Get());
    ID3D12DescriptorHeap* heaps[]{heap.Get()};
    using Heaps = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, ID3D12DescriptorHeap* const*);
    using Root = void(WINAPI*)(ID3D12GraphicsCommandList*, ID3D12RootSignature*);
    using Table = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, D3D12_GPU_DESCRIPTOR_HANDLE);
    method<Heaps>(list.Get(), 32)(list.Get(), 1, heaps);
    method<Root>(list.Get(), 33)(list.Get(), f.root.Get());
    method<Table>(list.Get(), 35)(list.Get(), 0, gpu(0));
    method<Table>(list.Get(), 35)(list.Get(), 1, gpu(64));
    list->Dispatch(1, 1, 1);
    barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    list->ResourceBarrier(1, &barrier);
    list->CopyBufferRegion(read.Get(), 0, out.Get(), 0, seed.size() * 4);
    submit(dev, f.queue.Get(), list.Get());
    D3D12_RANGE range{0, seed.size() * 4}, none{};
    void* data{};
    check(read->Map(0, &range, &data), "map result");
    const UINT* got = static_cast<const UINT*>(data);
    UINT expected_mask = 0, observed_mask = 0, bad = 0;
    float worst = 0;
    for (UINT i = 0; i < lanes; ++i) {
        const float t = i < count && s.beams[i].selected ? nearest(s.quads, s.beams[i]) : INFINITY;
        const bool hit = t != INFINITY;
        float observed;
        std::memcpy(&observed, &got[2 * i], 4);
        expected_mask |= UINT(hit) << i;
        observed_mask |= UINT(got[2 * i] != 0x7f7fffffu) << i;
        if (hit) {
            const float error = std::fabs(observed - (t + 0.04f));
            worst = std::fmax(worst, error);
            bad += !(error <= 1e-4f * std::fmax(1.f, t));
        } else {
            bad += got[2 * i] != 0x7f7fffffu;
        }
        bad += got[2 * i + 1] != 7;
    }
    std::fprintf(receipt, "laser_%s=expected %08x observed %08x mismatches %u worst %g\n", s.name, expected_mask,
                 observed_mask, bad, worst);
    if (bad) std::fprintf(receipt, "laser_%s_lane0=%08x %08x\n", s.name, got[0], got[1]);
    read->Unmap(0, &none);
    std::fflush(receipt);
    return bad == 0;
}

// Beams from a 4x8 grid at z = 0 along d, every lamp selected unless masked.
static std::vector<Beam> beams(float dx, float dy, float dz, float length, UINT unselected = 0) {
    std::vector<Beam> out;
    for (UINT i = 0; i < lanes; ++i)
        out.push_back({{-1.75f + 0.5f * float(i % 8), -0.75f + 0.5f * float(i / 8), 0.f},
                       {dx, dy, dz},
                       length,
                       !(unselected >> i & 1)});
    return out;
}

static int laser(const wchar_t* shader, FILE* receipt) {
    // The registers the program declares, at the slots it reads.
    D3D12_DESCRIPTOR_RANGE first[]{{D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 47, 0, 5}};
    D3D12_DESCRIPTOR_RANGE second[]{{D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 371, 1, 0},
                                    {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 354, 1, 1},
                                    {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 48, 1, 2}};
    D3D12_ROOT_PARAMETER params[2]{};
    params[0].ParameterType = params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    params[0].DescriptorTable = {1, first};
    params[1].DescriptorTable = {3, second};
    const D3D12_ROOT_SIGNATURE_DESC desc{2, params, 0, nullptr, D3D12_ROOT_SIGNATURE_FLAG_NONE};
    Fixture f(shader, &desc);
    const Quad wall{5.f, -100, 100, -100, 100}, near_wall{3.f, -100, 100, -100, 100};
    const Quad behind{-5.f, -100, 100, -100, 100}, left{4.f, -100, 0, -100, 100};
    const std::vector<LaserScenario> scenarios{
        {"wall", {wall}, beams(0, 0, 1, 10)},
        {"nearest_of_two", {wall, near_wall}, beams(0, 0, 1, 10)},
        {"nearest_of_two_reversed", {near_wall, wall}, beams(0, 0, 1, 10)},
        {"beyond_length", {wall}, beams(0, 0, 1, 4)},
        {"behind", {behind}, beams(0, 0, 1, 10)},
        {"empty", {}, beams(0, 0, 1, 10)},
        {"half", {left, wall}, beams(0, 0, 1, 10)},
        {"unnormalised", {wall}, beams(0, 0, 2, 10)},
        {"oblique", {wall}, beams(0.5f, 0, 1, 10)},
        {"unselected", {wall}, beams(0, 0, 1, 10, 0xf0f0000fu)},
    };
    int failed = 0;
    for (const auto& s : scenarios) failed += !run_laser(f, s, receipt);
    return failed;
}

// CS_RaytraceShadows_Mixed (mode "sun") is recorded without a root signature
// either. It refines a screen tile's packed shadow words: each selected pixel
// rebuilds its position from depth and a view ray, then traces toward the sun.
// The guest chooses inputs under which that position is exact:
// - root constants b500: x view 0, y nonzero (skip the cascade test), z the
//   maximum distance, w zero to trace only the second scene, nonzero to trace
//   the first and, after a miss, the second.
// - t405 (table 1, slot 2): tile word; 31 << 27 selects threads 0..30.
// - t408 (slot 3): pixel words x | y << 16. Rows start at 128 so that the
//   words are normal floats in a float-typed buffer.
// - t330 (slot 6): G-buffer word; 0x88800800 marks a lit surface facing z.
// - t331 (slot 7): depth 1.
// - b17 (table 0, slot 129): .x of rows 0-5 is the row length 8, which also
//   turns the sample jitter off.
// - t16 (slot 5): the view ray of pixel (x, y) is (u, v, 0) with
//   u = (x + 0.5) / 4 - 1 and v = 64 + 1 - (y + 0.5) / 2.
// - t10 (slot 0): byte 272 holds L = (0, 0, -1); rays run along +z.
// - t354 (slot 4) is the first scene, t355 (slot 1) the second.
// - u40 (slot 8): word 3 * (x + 8 * y) is the previous shadow word; its low
//   15 bits (f16, scaled by 0.01) are the minimum distance, here 0. An
//   occluded pixel gets bit 15 set; others keep their word.
struct SunScenario {
    const char* name;
    UINT both;  // root constant w
    float tmax;
    std::vector<Quad> first, second;
};

static ComPtr<ID3D12Resource> texture(Fixture& f, UINT width, UINT height, const std::vector<UINT>& texels) {
    auto dev = f.dev.Get();
    D3D12_HEAP_PROPERTIES props{};
    props.Type = D3D12_HEAP_TYPE_DEFAULT;
    props.CreationNodeMask = props.VisibleNodeMask = 1;
    D3D12_RESOURCE_DESC desc{};
    desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    desc.Width = width;
    desc.Height = height;
    desc.DepthOrArraySize = desc.MipLevels = desc.SampleDesc.Count = 1;
    desc.Format = DXGI_FORMAT_R32_FLOAT;
    ComPtr<ID3D12Resource> tex;
    check(dev->CreateCommittedResource(&props, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COPY_DEST, nullptr,
                                       IID_PPV_ARGS(&tex)),
          "create texture");
    constexpr UINT pitch = 256;
    std::vector<UINT> rows(pitch / 4 * height);
    for (UINT y = 0; y < height; ++y)
        for (UINT x = 0; x < width; ++x) rows[pitch / 4 * y + x] = texels[width * y + x];
    auto staging = upload(dev, rows.data(), rows.size() * 4);
    ComPtr<ID3D12CommandAllocator> alloc;
    check(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc)), "texture allocator");
    auto list = guest_list(dev, alloc.Get());
    D3D12_TEXTURE_COPY_LOCATION to{tex.Get(), D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX, {}};
    D3D12_TEXTURE_COPY_LOCATION from{staging.Get(), D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT, {}};
    from.PlacedFootprint.Footprint = {DXGI_FORMAT_R32_FLOAT, width, height, 1, pitch};
    list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    D3D12_RESOURCE_BARRIER barrier{};
    barrier.Transition = {tex.Get(), 0, D3D12_RESOURCE_STATE_COPY_DEST,
                          D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE};
    list->ResourceBarrier(1, &barrier);
    submit(dev, f.queue.Get(), list.Get());
    return tex;
}

static bool run_sun(Fixture& f, const SunScenario& s, FILE* receipt) {
    auto dev = f.dev.Get();
    const auto first = f.scene(s.first);
    const auto second = f.scene(s.second);
    D3D12_DESCRIPTOR_HEAP_DESC hd{D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 256, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, 0};
    ComPtr<ID3D12DescriptorHeap> heap;
    check(dev->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&heap)), "view descriptors");
    const auto step = dev->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    using Gpu = UINT64(WINAPI*)(ID3D12DescriptorHeap*);
    auto cpu = [&](UINT i) {
        return D3D12_CPU_DESCRIPTOR_HANDLE{method<frame_abi::CpuHandle>(heap.Get(), 9)(heap.Get()) + i * step};
    };
    auto gpu = [&](UINT i) { return D3D12_GPU_DESCRIPTOR_HANDLE{method<Gpu>(heap.Get(), 10)(heap.Get()) + i * step}; };
    constexpr UINT table1 = 192, width = 8, top = 128, height = top + lanes / width;
    auto typed = [&](UINT slot, ID3D12Resource* res, UINT count) {
        D3D12_SHADER_RESOURCE_VIEW_DESC view{};
        view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        view.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
        view.Format = DXGI_FORMAT_R32_FLOAT;
        view.Buffer.NumElements = count;
        dev->CreateShaderResourceView(res, &view, cpu(slot));
    };
    auto structured = [&](UINT slot, ID3D12Resource* res, UINT stride) {
        D3D12_SHADER_RESOURCE_VIEW_DESC view{};
        view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        view.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
        view.Buffer.StructureByteStride = stride;
        view.Buffer.NumElements = 1;
        dev->CreateShaderResourceView(res, &view, cpu(slot));
    };
    auto scene_view = [&](UINT slot, ID3D12Resource* res) {
        D3D12_SHADER_RESOURCE_VIEW_DESC view{};
        view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        view.ViewDimension = D3D12_SRV_DIMENSION_BUFFER;
        view.Format = DXGI_FORMAT_R32_TYPELESS;
        view.Buffer.Flags = D3D12_BUFFER_SRV_FLAG_RAW;
        view.Buffer.NumElements = 32;
        dev->CreateShaderResourceView(res, &view, cpu(slot));
    };
    auto texel = [&](UINT slot, ID3D12Resource* res) {
        D3D12_SHADER_RESOURCE_VIEW_DESC view{};
        view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        view.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
        view.Format = DXGI_FORMAT_R32_FLOAT;
        view.Texture2D.MipLevels = 1;
        dev->CreateShaderResourceView(res, &view, cpu(slot));
    };
    std::vector<UINT> light(85);
    light[272 / 4 + 2] = bits(-1.f);
    auto light_buffer = upload(dev, light.data(), light.size() * 4);
    structured(table1 + 0, light_buffer.Get(), 340);
    scene_view(table1 + 1, second.top.Get());
    const UINT tile[4]{31u << 27, 0, 0, 0};
    auto tile_buffer = upload(dev, tile, sizeof(tile));
    typed(table1 + 2, tile_buffer.Get(), 1);
    std::vector<UINT> pixels(lanes);
    for (UINT i = 0; i < lanes; ++i) pixels[i] = i % width | (top + i / width) << 16;
    auto pixel_buffer = upload(dev, pixels.data(), pixels.size() * 4);
    typed(table1 + 3, pixel_buffer.Get(), lanes);
    scene_view(table1 + 4, first.top.Get());
    std::vector<UINT> view_words(72);
    view_words[16 / 4] = bits(1.f);   // u from the x ramp
    view_words[32 / 4] = bits(1.f);   // v from the y ramp
    view_words[36 / 4] = bits(64.f);  // ... moved back to the origin
    view_words[76 / 4] = bits(1.f / 8);
    view_words[80 / 4] = bits(0.25f);
    auto view_buffer = upload(dev, view_words.data(), view_words.size() * 4);
    structured(table1 + 5, view_buffer.Get(), 288);
    auto gbuffer = texture(f, width, height, std::vector<UINT>(width * height, 0x88800800u));
    texel(table1 + 6, gbuffer.Get());
    auto depth = texture(f, width, height, std::vector<UINT>(width * height, bits(1.f)));
    texel(table1 + 7, depth.Get());
    const UINT words = 3 * width * height;
    std::vector<UINT> seed(words);
    for (UINT i = 0; i < words; ++i) seed[i] = 0x40000000u | (i % 2048) << 16;
    auto out = buffer(dev, words * 4, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COPY_DEST, true);
    auto read = buffer(dev, words * 4, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
    auto seed_buffer = upload(dev, seed.data(), words * 4);
    D3D12_UNORDERED_ACCESS_VIEW_DESC uav{};
    uav.ViewDimension = D3D12_UAV_DIMENSION_BUFFER;
    uav.Format = DXGI_FORMAT_R32_FLOAT;
    uav.Buffer.NumElements = words;
    dev->CreateUnorderedAccessView(out.Get(), nullptr, &uav, cpu(table1 + 8));
    std::vector<UINT> constants(256);
    auto cb20 = upload(dev, constants.data(), constants.size() * 4);
    D3D12_CONSTANT_BUFFER_VIEW_DESC cbv{cb20->GetGPUVirtualAddress(), 1024};
    dev->CreateConstantBufferView(&cbv, cpu(table1 + 9));
    std::vector<UINT> rows(65536 / 4);
    for (UINT r = 0; r < 6; ++r) rows[4 * r] = bits(float(width));
    auto cb17 = upload(dev, rows.data(), rows.size() * 4);
    D3D12_CONSTANT_BUFFER_VIEW_DESC wide{cb17->GetGPUVirtualAddress(), 65536};
    dev->CreateConstantBufferView(&wide, cpu(129));

    ComPtr<ID3D12CommandAllocator> alloc;
    check(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc)), "allocator");
    auto list = guest_list(dev, alloc.Get());
    list->CopyBufferRegion(out.Get(), 0, seed_buffer.Get(), 0, words * 4);
    D3D12_RESOURCE_BARRIER barrier{};
    barrier.Transition = {out.Get(), 0, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS};
    list->ResourceBarrier(1, &barrier);
    list->SetPipelineState(f.pipeline.Get());
    ID3D12DescriptorHeap* heaps[]{heap.Get()};
    using Heaps = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, ID3D12DescriptorHeap* const*);
    using Root = void(WINAPI*)(ID3D12GraphicsCommandList*, ID3D12RootSignature*);
    using Table = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, D3D12_GPU_DESCRIPTOR_HANDLE);
    using Constants = void(WINAPI*)(ID3D12GraphicsCommandList*, UINT, UINT, const void*, UINT);
    method<Heaps>(list.Get(), 32)(list.Get(), 1, heaps);
    method<Root>(list.Get(), 33)(list.Get(), f.root.Get());
    method<Table>(list.Get(), 35)(list.Get(), 0, gpu(0));
    method<Table>(list.Get(), 35)(list.Get(), 1, gpu(table1));
    const UINT index[4]{0, 1, UINT(s.tmax), s.both};
    method<Constants>(list.Get(), 39)(list.Get(), 2, 4, index, 0);
    list->Dispatch(1, 1, 1);
    barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    list->ResourceBarrier(1, &barrier);
    list->CopyBufferRegion(read.Get(), 0, out.Get(), 0, words * 4);
    submit(dev, f.queue.Get(), list.Get());
    D3D12_RANGE range{0, words * 4}, none{};
    void* data{};
    check(read->Map(0, &range, &data), "map result");
    const UINT* got = static_cast<const UINT*>(data);
    std::vector<UINT> want = seed;
    UINT expected_mask = 0, observed_mask = 0, bad = 0;
    for (UINT i = 0; i < lanes; ++i) {
        const UINT x = i % width, y = top + i / width, slot = 3 * (x + width * y);
        const Beam ray{{(float(x) + 0.5f) / 4 - 1, 65 - (float(y) + 0.5f) / 2, 0.f}, {0, 0, 1}, s.tmax, i < 31};
        const auto hit_in = [&](const std::vector<Quad>& quads) { return nearest(quads, ray) != INFINITY; };
        const bool hit = ray.selected && ((s.both && hit_in(s.first)) || hit_in(s.second));
        if (hit) want[slot] |= 0x8000u;
        expected_mask |= UINT(hit) << i;
        observed_mask |= UINT(got[slot] != seed[slot]) << i;
    }
    for (UINT i = 0; i < words; ++i) bad += got[i] != want[i];
    UINT first_bad = words;
    for (UINT i = 0; i < words && first_bad == words; ++i)
        if (got[i] != want[i]) first_bad = i;
    std::fprintf(receipt, "sun_%s=expected %08x observed %08x mismatches %u\n", s.name, expected_mask, observed_mask,
                 bad);
    if (bad) std::fprintf(receipt, "sun_%s_first=%u want %08x got %08x\n", s.name, first_bad, want[first_bad],
                          got[first_bad]);
    read->Unmap(0, &none);
    std::fflush(receipt);
    return bad == 0;
}

static int sun(const wchar_t* shader, FILE* receipt) {
    D3D12_DESCRIPTOR_RANGE first[]{{D3D12_DESCRIPTOR_RANGE_TYPE_CBV, 1, 17, 0, 129}};
    D3D12_DESCRIPTOR_RANGE second[]{
        {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 10, 1, 0},  {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 355, 1, 1},
        {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 405, 1, 2}, {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 408, 1, 3},
        {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 354, 1, 4}, {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 16, 1, 5},
        {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 330, 1, 6}, {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 331, 1, 7},
        {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 40, 1, 8},  {D3D12_DESCRIPTOR_RANGE_TYPE_CBV, 1, 20, 1, 9}};
    D3D12_ROOT_PARAMETER params[3]{};
    params[0].ParameterType = params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    params[0].DescriptorTable = {1, first};
    params[1].DescriptorTable = {10, second};
    params[2].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    params[2].Constants = {500, 0, 4};
    const D3D12_ROOT_SIGNATURE_DESC desc{3, params, 0, nullptr, D3D12_ROOT_SIGNATURE_FLAG_NONE};
    Fixture f(shader, &desc);
    const Quad wall{5.f, -100, 100, -100, 100}, behind{-5.f, -100, 100, -100, 100};
    const Quad left{5.f, -100, 0, -100, 100}, upper{4.f, -100, 100, 0, 100};
    const std::vector<SunScenario> scenarios{
        {"first_occluder", 1, 10.f, {wall}, {}},
        {"second_occluder", 1, 10.f, {}, {wall}},
        {"nothing", 1, 10.f, {}, {}},
        {"beyond_maximum", 1, 3.f, {wall}, {wall}},
        {"behind", 1, 10.f, {behind}, {behind}},
        {"first_half", 1, 10.f, {left}, {}},
        {"both_halves", 1, 10.f, {left}, {upper}},
        {"second_only_skips_first", 0, 10.f, {wall}, {}},
        {"second_only_occluder", 0, 10.f, {}, {wall}},
        {"second_only_half", 0, 10.f, {left}, {upper}},
    };
    int failed = 0;
    for (const auto& s : scenarios) failed += !run_sun(f, s, receipt);
    return failed;
}

int wmain(int argc, wchar_t** argv) {
    if (argc != 4 || (wcscmp(argv[2], L"near") && wcscmp(argv[2], L"far") && wcscmp(argv[2], L"laser") &&
                      wcscmp(argv[2], L"sun")))
        return 2;
    FILE* receipt{};
    if (_wfopen_s(&receipt, argv[3], L"wb")) return 2;
    const bool with_far = !wcscmp(argv[2], L"far");
    const char* label = with_far ? "shadow_far" : "shadow";
    int failed = 0;
    try {
        if (!wcscmp(argv[2], L"laser") || !wcscmp(argv[2], L"sun")) {
            failed = !wcscmp(argv[2], L"laser") ? laser(argv[1], receipt) : sun(argv[1], receipt);
            std::fprintf(receipt, "failed=%d\n", failed);
            std::fclose(receipt);
            return failed ? 1 : 0;
        }
        Fixture f(argv[1]);
        LUID luid = f.dev->GetAdapterLuid();
        ComPtr<IDXGIFactory4> factory;
        ComPtr<IDXGIAdapter1> adapter;
        DXGI_ADAPTER_DESC1 desc{};
        if (SUCCEEDED(CreateDXGIFactory1(IID_PPV_ARGS(&factory))) &&
            SUCCEEDED(factory->EnumAdapterByLuid(luid, IID_PPV_ARGS(&adapter))))
            adapter->GetDesc1(&desc);
        std::fprintf(receipt, "adapter=%04x:%04x\n", desc.VendorId, desc.DeviceId);
        const Quad wall{5.f, -100, 100, -100, 100}, behind{-5.f, -100, 100, -100, 100};
        const Quad lower{5.f, -100, 100, -100, 0}, inside{2.f, -100, 100, -100, 100};
        std::vector<Scenario> scenarios{
            {"occluder", 10.f, 0.f, {wall}, {}},
            {"behind", 10.f, 0.f, {behind}, {}},
            {"beyond_maximum", 3.f, 0.f, {wall}, {}},
            {"empty", 10.f, 0.f, {}, {}},
            {"half_occluder", 10.f, 0.f, {lower}, {}},
        };
        if (with_far) {
            scenarios.push_back({"far_occluder", 10.f, 0.f, {}, {wall}});
            scenarios.push_back({"far_behind", 10.f, 0.f, {}, {behind}});
            scenarios.push_back({"far_half", 10.f, 0.f, {}, {lower}});
            // Near radius 3: the far walk starts about 3 along the ray and
            // skips far geometry inside the sphere, but reaches beyond it.
            scenarios.push_back({"far_inside_near_sphere", 10.f, 3.f / 0.7f, {}, {inside}});
            scenarios.push_back({"far_beyond_near_sphere", 10.f, 3.f / 0.7f, {}, {wall}});
            scenarios.push_back({"near_inside_sphere", 10.f, 3.f / 0.7f, {inside}, {}});
            scenarios.push_back({"far_past_shortened_maximum", 5.5f, 3.f / 0.7f, {}, {wall}});
        }
        for (const auto& s : scenarios) failed += !run(f, s, with_far, receipt, label);
    } catch (const std::exception& e) {
        std::fprintf(receipt, "error=%s\n", e.what());
        failed = 100;
    }
    std::fprintf(receipt, "failed=%d\n", failed);
    std::fclose(receipt);
    return failed ? 1 : 0;
}
