//! A copy of title memory in video memory.
//!
//! [`crate::shadow`] keeps its copies in system memory ([`Host::shared_buffer`],
//! a `CUSTOM`/`L0` buffer): the title's memory is host memory, so the copy is
//! a duplicate of it that the CPU can refresh and compare, and every shader
//! read of it crosses PCIe. On the console the same read is GDDR6.
//!
//! This is the other arrangement, prepared and tested here and not wired in
//! yet: the copy shaders read is a buffer in video memory, brought up to date
//! by a copy per changed range (staged through upload memory) and read through
//! a readback buffer when the GPU wrote it.
//!
//! What the integration needs, in `shadow.rs` and `host.rs`:
//!
//! * `Shadow` gains `Option<VramCopy>`; `gpu()` returns its address where it
//!   has one. Video memory is worth it only for copies read far more than
//!   written and only within a budget - a copy of every title buffer would
//!   not fit beside the title's own resources;
//! * `Batch::flush` (which memcpys title memory into the mapped copy today)
//!   stages each range with [`VramCopy::upload`] instead, which records the
//!   copy on the list, so it runs before the list's shaders do;
//! * `Shadow::copy_back_page`, which compares the mapped copy, takes the page
//!   from [`VramCopy::read`] instead;
//! * the CPU copy stays: it is what the title's own threads read, what a
//!   refresh compares against, and what `crate::memory` counts.
//!
//! The tests are the semantics that has to hold for that to be safe: bytes
//! uploaded reach a shader that reads them by GPU address, what a shader
//! writes reads back, and two ranges of one list do not overwrite each other.

#![allow(dead_code)] // wired into `shadow` by the next change; the tests drive it.

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

use crate::host::{Host, HostList, host, transition};

/// The state the copy is left in between its uses: common, which a shader's
/// root view and a copy both promote, and which every copy here enters and
/// leaves explicitly (a copy that starts and ends in it is what orders it
/// against the shader access on either side).
const COPY_BETWEEN: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATE_COMMON;

/// A copy of title memory in video memory.
pub(crate) struct VramCopy {
    buffer: ID3D12Resource,
    /// Where it is, in the title's address space.
    gpu: u64,
    len: u64,
}

// SAFETY: D3D12 resources are free-threaded.
unsafe impl Send for VramCopy {}
// SAFETY: as above.
unsafe impl Sync for VramCopy {}

impl VramCopy {
    /// `len` bytes of video memory (cut up to 64 KiB: copies and views start
    /// on whole elements), or `None` when the host will not give them.
    pub(crate) fn new(len: u64) -> Option<VramCopy> {
        let host = host()?;
        let len = len.max(1).next_multiple_of(1 << 16);
        let buffer = vram_buffer(host, len)?;
        crate::host::name(&buffer, || format!("title memory copy (video) {len:#x}"));
        // SAFETY: a live buffer.
        let gpu = unsafe { buffer.GetGPUVirtualAddress() };
        (gpu != 0).then_some(VramCopy { buffer, gpu, len })
    }

    /// The address a shader reads this copy at.
    pub(crate) fn gpu(&self) -> u64 {
        self.gpu
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// `bytes` are title memory at `offset` in the copy: written to upload
    /// memory now, and copied into video memory when `list` runs (so the
    /// list's own shaders read them, and not before).
    ///
    /// `None` when the range does not fit or the host has no upload memory;
    /// nothing is recorded then.
    pub(crate) fn upload(&self, list: &HostList, offset: u64, bytes: &[u8]) -> Option<()> {
        let len = bytes.len() as u64;
        if len == 0 || offset.checked_add(len)? > self.len {
            return None;
        }
        let host = host()?;
        let (staging, mapped) = list.upload(host, len)?;
        // SAFETY: our mapped upload buffer of `len` bytes, kept by the list
        // until it is reset - after the copy has run.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped, bytes.len()) };
        // SAFETY: an open list and live buffers. The copy destination is
        // named and given back, as a copy of an indirect argument buffer is:
        // a copy between two states is what orders it against the shader
        // access before and after (a bare copy from common is not).
        unsafe {
            let l = &list.list;
            l.ResourceBarrier(&[transition(&self.buffer, COPY_BETWEEN, D3D12_RESOURCE_STATE_COPY_DEST)]);
            l.CopyBufferRegion(&self.buffer, offset, &staging, 0, len);
            l.ResourceBarrier(&[transition(&self.buffer, D3D12_RESOURCE_STATE_COPY_DEST, COPY_BETWEEN)]);
        }
        Some(())
    }

    /// A readback of `[offset, offset + len)` as `list` leaves it, read after
    /// the list has run ([`Read::bytes`]).
    pub(crate) fn read(&self, list: &HostList, offset: u64, len: u64) -> Option<Read> {
        if len == 0 || offset.checked_add(len)? > self.len {
            return None;
        }
        let host = host()?;
        let (readback, mapped) = list.readback(host, len)?;
        // SAFETY: an open list and live buffers, as above: the copy source is
        // named and given back, so the read sees what ran before it.
        unsafe {
            let l = &list.list;
            l.ResourceBarrier(&[transition(&self.buffer, COPY_BETWEEN, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
            l.CopyBufferRegion(&readback, 0, &self.buffer, offset, len);
            l.ResourceBarrier(&[transition(&self.buffer, D3D12_RESOURCE_STATE_COPY_SOURCE, COPY_BETWEEN)]);
        }
        Some(Read { buffer: readback, data: mapped.cast(), len })
    }
}

/// A recorded readback: the bytes once the list that recorded it has run.
pub(crate) struct Read {
    /// Held so the address names nothing else before the read.
    buffer: ID3D12Resource,
    data: *mut u8,
    len: u64,
}

// SAFETY: D3D12 resources are free-threaded; `data` is read after the list
// that wrote it has run.
unsafe impl Send for Read {}

impl Read {
    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// The bytes the GPU left in the range.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        // SAFETY: a mapped readback buffer of `len` bytes, written by the
        // copy the list recorded.
        unsafe { std::slice::from_raw_parts(self.data, self.len as usize) }.to_vec()
    }
}

/// A buffer in video memory that shaders read and write: unordered access
/// allowed, common, so a copy and a root view promote it as they need.
fn vram_buffer(host: &Host, size: u64) -> Option<ID3D12Resource> {
    let heap = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() };
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: size.max(1),
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        ..Default::default()
    };
    let mut buffer: Option<ID3D12Resource> = None;
    // SAFETY: plain creation of a committed buffer.
    let made = unsafe {
        host.device
            .CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COMMON, None, &mut buffer)
    };
    made.ok().and(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `dst = src * 2` and `dst + 4 = src + 4 + 1`, so a read says both which
    /// bytes arrived and that the shader, not the upload, wrote them.
    const SHADER: &str = r#"
#define RS "RootFlags(ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT), SRV(t0), UAV(u0)"
ByteAddressBuffer src : register(t0);
RWByteAddressBuffer dst : register(u0);
[RootSignature(RS)]
[numthreads(1, 1, 1)]
void main() {
    dst.Store(0, src.Load(0) * 2);
    dst.Store(4, src.Load(4) + 1);
}
"#;

    /// `bytes` as little-endian words: what the tests compare.
    fn words(bytes: &[u8]) -> Vec<u32> {
        let mut out = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks(4) {
            out.push(u32::from_le_bytes(chunk.try_into().expect("4")));
        }
        out
    }

    struct Copy {
        root: ID3D12RootSignature,
        pso: ID3D12PipelineState,
    }

    /// The test's compute pipeline, or `None` where the host has no GPU or
    /// no shader compiler.
    fn pipeline() -> Option<Copy> {
        use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
        use windows::Win32::Graphics::Direct3D::ID3DBlob;
        use windows::core::s;
        let host = host()?;
        let (mut code, mut errors) = (None::<ID3DBlob>, None::<ID3DBlob>);
        // SAFETY: the source lives through the call; out pointers ours.
        let compiled = unsafe {
            D3DCompile(
                SHADER.as_ptr().cast(),
                SHADER.len(),
                s!("copy"),
                None,
                None,
                s!("main"),
                s!("cs_5_1"),
                0,
                0,
                &mut code,
                Some(&mut errors),
            )
        };
        compiled.ok()?;
        let code = code?;
        // SAFETY: the blob's own bytes; the root signature is in the shader.
        unsafe {
            let bytes = std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize());
            let root: ID3D12RootSignature = host.device.CreateRootSignature(0, bytes).ok()?;
            let pso = host
                .device
                .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&root),
                    CS: D3D12_SHADER_BYTECODE {
                        pShaderBytecode: bytes.as_ptr().cast(),
                        BytecodeLength: bytes.len(),
                    },
                    ..Default::default()
                })
                .ok()?;
            Some(Copy { root, pso })
        }
    }

    /// Records the test's dispatch over `src` and `dst`.
    fn dispatch(list: &HostList, copy: &Copy, src: u64, dst: u64) {
        let l = &list.list;
        // SAFETY: an open list; our root signature, pipeline and buffers.
        unsafe {
            l.SetComputeRootSignature(&copy.root);
            l.SetPipelineState(&copy.pso);
            l.SetComputeRootShaderResourceView(0, src);
            l.SetComputeRootUnorderedAccessView(1, dst);
            l.Dispatch(1, 1, 1);
        }
    }

    /// Runs `record` on one list and returns what `read` gives after it.
    fn run<T>(record: impl FnOnce(&HostList, &Copy) -> T) -> Option<T> {
        let host = host()?;
        let copy = pipeline()?;
        let list = host.new_list().ok()?;
        let out = record(&list, &copy);
        list.close();
        host.execute(&[&list]);
        Some(out)
    }

    /// What the host uploads reaches a shader reading the copy at its GPU
    /// address, and the shader's result reads back: the two ends of the
    /// arrangement, in one run.
    #[test]
    fn uploaded_bytes_reach_a_shader_and_its_result_reads_back() {
        let Some((src, dst)) = host().and_then(|_| Some((VramCopy::new(64 << 10)?, VramCopy::new(64 << 10)?)))
        else {
            return; // no host GPU here
        };
        let pattern: [u32; 2] = [7, 41];
        let bytes: Vec<u8> = pattern.iter().flat_map(|v| v.to_le_bytes()).collect();
        let read = run(|list, copy| {
            src.upload(list, 128, &bytes).expect("an upload");
            dispatch(list, copy, src.gpu() + 128, dst.gpu());
            // Recorded after the dispatch: this is what the shader left.
            dst.read(list, 0, 8).expect("a readback")
        });
        let Some(read) = read else {
            return; // no shader compiler here either
        };
        let out = words(&read.bytes());
        assert_eq!(out, [14, 42], "the shader read what the host uploaded");
    }

    /// What a shader writes into the copy is what a later read gives - the
    /// write-back path's view of the page.
    #[test]
    fn what_a_shader_writes_reads_back() {
        let Some((src, dst)) = host().and_then(|_| Some((VramCopy::new(64 << 10)?, VramCopy::new(64 << 10)?)))
        else {
            return;
        };
        let pattern: [u32; 2] = [100, 9];
        let bytes: Vec<u8> = pattern.iter().flat_map(|v| v.to_le_bytes()).collect();
        let read = run(|list, copy| {
            src.upload(list, 0, &bytes).expect("an upload");
            dispatch(list, copy, src.gpu(), dst.gpu() + 512);
            dst.read(list, 512, 8).expect("a readback")
        });
        let Some(read) = read else {
            return;
        };
        let out = words(&read.bytes());
        assert_eq!(out, [200, 10]);
    }

    /// The upload and the readback alone, with no shader between them: the
    /// bytes go into video memory and come back.
    #[test]
    fn an_upload_reads_back_without_a_shader() {
        let Some(copy) = host().and_then(|_| VramCopy::new(64 << 10)) else {
            return;
        };
        let host = super::host().expect("a host GPU");
        let list = host.new_list().expect("a list");
        let bytes: Vec<u8> = (0..64u32).flat_map(|v| v.wrapping_mul(7).to_le_bytes()).collect();
        copy.upload(&list, 256, &bytes).expect("an upload");
        let read = copy.read(&list, 256, bytes.len() as u64).expect("a readback");
        list.close();
        host.execute(&[&list]);
        assert_eq!(read.bytes(), bytes, "the bytes of the copy");
    }

    /// Two ranges of one list land where they were asked for: a staged upload
    /// is not shared behind the caller's back.
    #[test]
    fn two_ranges_in_one_list_do_not_overwrite_each_other() {
        let Some(copy) = host().and_then(|_| VramCopy::new(64 << 10)) else {
            return;
        };
        let (first, second) = (0xdead_beef_u32.to_le_bytes(), 0x1234_5678_u32.to_le_bytes());
        let host = super::host().expect("a host GPU");
        let list = host.new_list().expect("a list");
        copy.upload(&list, 0, &first).expect("the first range");
        copy.upload(&list, 4096, &second).expect("the second range");
        let reads = (copy.read(&list, 0, 4).expect("read one"), copy.read(&list, 4096, 4).expect("read two"));
        list.close();
        host.execute(&[&list]);
        assert_eq!(reads.0.bytes(), first);
        assert_eq!(reads.1.bytes(), second);
    }

    /// A range that does not fit the copy is refused, and nothing is
    /// recorded for it.
    #[test]
    fn a_range_past_the_end_is_refused() {
        let Some(copy) = host().and_then(|_| VramCopy::new(1 << 16)) else {
            return;
        };
        let host = super::host().expect("a host GPU");
        let list = host.new_list().expect("a list");
        let len = copy.len();
        let byte = [0u8; 4];
        assert!(copy.upload(&list, len - 4, &byte).is_some(), "the last element fits");
        assert!(copy.upload(&list, len - 2, &byte).is_none(), "one past the end");
        assert!(copy.upload(&list, 0, &[]).is_none(), "nothing to copy");
        assert!(copy.read(&list, len - 4, 8).is_none(), "past the end");
        assert!(copy.read(&list, 0, 0).is_none(), "nothing to read");
        list.close();
    }

    /// A copy's length is a whole number of 64 KiB pages, which is what
    /// views and copies inside it start on.
    #[test]
    fn a_copy_is_whole_pages() {
        let Some(copy) = host().and_then(|_| VramCopy::new(1)) else {
            return;
        };
        assert_eq!(copy.len(), 1 << 16);
        assert_eq!(copy.gpu() & 0xff, 0, "a buffer's address is 256-byte aligned");
    }
}
