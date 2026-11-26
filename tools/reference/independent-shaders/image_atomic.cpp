// Dispatch translated compute shaders on a 4x4x4 R32_UINT 3D texture (u0) and a
// 96-word R32_UINT buffer (u1), both seeded from <case>.seed. <case>.bin holds the
// texels, x fastest, then the buffer.
#include "context.h"

using namespace shader_control;

constexpr UINT SIDE = 4, TEXELS = SIDE * SIDE * SIDE, WORDS = 96;
constexpr UINT PITCH = D3D12_TEXTURE_DATA_PITCH_ALIGNMENT, SLICE = PITCH * SIDE;

static void dispatch(Context &ctx, FILE *receipt, const std::string &name) {
    ID3D12Device *device = ctx.device.Get();
    D3D12_DESCRIPTOR_RANGE range{D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 2, 0, 0, 0};
    D3D12_ROOT_PARAMETER table{};
    table.ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    table.DescriptorTable = {1, &range};
    D3D12_ROOT_SIGNATURE_DESC description{1, &table, 0, nullptr, D3D12_ROOT_SIGNATURE_FLAG_NONE};
    ComPtr<ID3DBlob> serialized;
    check(D3D12SerializeRootSignature(&description, D3D_ROOT_SIGNATURE_VERSION_1, &serialized, nullptr), "serialize root");
    ComPtr<ID3D12RootSignature> root;
    check(device->CreateRootSignature(0, serialized->GetBufferPointer(), serialized->GetBufferSize(),
                                      IID_PPV_ARGS(&root)), "create root");
    const auto code = read_shader(name);
    D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = root.Get();
    pd.CS = {code.data(), code.size()};
    ComPtr<ID3D12PipelineState> pipeline;
    check(device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pipeline)), "compute pipeline");

    std::vector<UINT> initial(TEXELS + WORDS);
    std::ifstream input(name + ".seed", std::ios::binary);
    const std::streamsize bytes = std::streamsize(initial.size() * sizeof(UINT));
    input.read(reinterpret_cast<char *>(initial.data()), bytes);
    if (input.gcount() != bytes || input.peek() != std::char_traits<char>::eof())
        throw std::runtime_error("invalid seed " + name);

    // Texel rows are padded to the copy pitch; the buffer follows the last slice.
    const UINT staging_bytes = SLICE * SIDE + WORDS * sizeof(UINT);
    std::vector<unsigned char> staged(staging_bytes);
    for (UINT i = 0; i < TEXELS; ++i)
        std::memcpy(staged.data() + (i / 16) * SLICE + (i / 4 % 4) * PITCH + (i % 4) * 4, &initial[i], 4);
    std::memcpy(staged.data() + SLICE * SIDE, initial.data() + TEXELS, WORDS * sizeof(UINT));
    auto seed = buffer(device, staging_bytes, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
    auto readback = buffer(device, staging_bytes, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
    upload(seed.Get(), staged.data(), staging_bytes);
    auto out = buffer(device, WORDS * sizeof(UINT), D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COPY_DEST,
                      D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);

    D3D12_HEAP_PROPERTIES props{};
    props.Type = D3D12_HEAP_TYPE_DEFAULT;
    props.VisibleNodeMask = props.CreationNodeMask = 1;
    D3D12_RESOURCE_DESC td{};
    td.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    td.Width = td.Height = td.DepthOrArraySize = SIDE;
    td.MipLevels = 1;
    td.Format = DXGI_FORMAT_R32_UINT;
    td.SampleDesc.Count = 1;
    td.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    ComPtr<ID3D12Resource> texture;
    check(device->CreateCommittedResource(&props, D3D12_HEAP_FLAG_NONE, &td, D3D12_RESOURCE_STATE_COPY_DEST, nullptr,
                                          IID_PPV_ARGS(&texture)), "create texture");

    D3D12_DESCRIPTOR_HEAP_DESC hd{D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 2, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, 0};
    ComPtr<ID3D12DescriptorHeap> heap;
    check(device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&heap)), "create UAV heap");
    auto cpu = heap->GetCPUDescriptorHandleForHeapStart();
    D3D12_UNORDERED_ACCESS_VIEW_DESC tv{};
    tv.Format = DXGI_FORMAT_R32_UINT;
    tv.ViewDimension = D3D12_UAV_DIMENSION_TEXTURE3D;
    tv.Texture3D = {0, 0, SIDE};
    device->CreateUnorderedAccessView(texture.Get(), nullptr, &tv, cpu);
    cpu.ptr += device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    D3D12_UNORDERED_ACCESS_VIEW_DESC bv{};
    bv.Format = DXGI_FORMAT_R32_UINT;
    bv.ViewDimension = D3D12_UAV_DIMENSION_BUFFER;
    bv.Buffer.NumElements = WORDS;
    device->CreateUnorderedAccessView(out.Get(), nullptr, &bv, cpu);

    D3D12_COMMAND_QUEUE_DESC qd{};
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> list;
    check(device->CreateCommandAllocator(qd.Type, IID_PPV_ARGS(&allocator)), "allocator");
    check(device->CreateCommandList(0, qd.Type, allocator.Get(), pipeline.Get(), IID_PPV_ARGS(&list)), "list");
    D3D12_TEXTURE_COPY_LOCATION staging{}, image{};
    staging.pResource = seed.Get();
    staging.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    staging.PlacedFootprint.Footprint = {DXGI_FORMAT_R32_UINT, SIDE, SIDE, SIDE, PITCH};
    image.pResource = texture.Get();
    image.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    list->CopyTextureRegion(&image, 0, 0, 0, &staging, nullptr);
    list->CopyBufferRegion(out.Get(), 0, seed.Get(), SLICE * SIDE, WORDS * sizeof(UINT));
    transition(list.Get(), texture.Get(), D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    transition(list.Get(), out.Get(), D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    ID3D12DescriptorHeap *heaps[] = {heap.Get()};
    list->SetDescriptorHeaps(1, heaps);
    list->SetComputeRootSignature(root.Get());
    list->SetComputeRootDescriptorTable(0, heap->GetGPUDescriptorHandleForHeapStart());
    list->Dispatch(1, 1, 1);
    transition(list.Get(), texture.Get(), D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE);
    transition(list.Get(), out.Get(), D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE);
    staging.pResource = readback.Get();
    list->CopyTextureRegion(&staging, 0, 0, 0, &image, nullptr);
    list->CopyBufferRegion(readback.Get(), SLICE * SIDE, out.Get(), 0, WORDS * sizeof(UINT));
    submit_and_wait(device, ctx.queue.Get(), list.Get());

    D3D12_RANGE read{0, staging_bytes}, none{};
    void *mapped{};
    check(readback->Map(0, &read, &mapped), "map readback");
    const auto *bytes_out = static_cast<const unsigned char *>(mapped);
    std::vector<UINT> result(TEXELS + WORDS);
    for (UINT i = 0; i < TEXELS; ++i)
        std::memcpy(&result[i], bytes_out + (i / 16) * SLICE + (i / 4 % 4) * PITCH + (i % 4) * 4, 4);
    std::memcpy(result.data() + TEXELS, bytes_out + SLICE * SIDE, WORDS * sizeof(UINT));
    readback->Unmap(0, &none);
    std::ofstream output(name + ".bin", std::ios::binary);
    output.write(reinterpret_cast<const char *>(result.data()), std::streamsize(result.size() * sizeof(UINT)));
    output.close();
    if (!output) throw std::runtime_error("write readback " + name);
    std::fprintf(receipt, "%s_words=%u\n", name.c_str(), UINT(result.size()));
}

int main() {
    FILE *receipt{};
    if (fopen_s(&receipt, "result.txt", "wb")) return 2;
    int status = 0;
    try {
        Context ctx(receipt);
        std::ifstream cases("cases.txt");
        std::string name;
        while (cases >> name) dispatch(ctx, receipt, name);
        if (!cases.eof()) throw std::runtime_error("invalid cases");
    } catch (const std::exception &error) {
        std::fprintf(receipt, "error=%s\n", error.what());
        status = 1;
    }
    return std::fclose(receipt) ? 2 : status;
}
