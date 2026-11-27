//! A paging fence for newly created device-local buffers.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{E_FAIL, E_NOINTERFACE, E_NOTIMPL, HANDLE};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::{DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_UNSUPPORTED};
use windows::core::{Error, Interface, Result};

use super::memory_samples::{self, Allocation, Step};

pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("VICTORY_UAV_ENQUEUE_RESIDENT").is_some_and(|value| value == "1"))
}

pub(super) struct Queue {
    device: ID3D12Device3,
    fence: ID3D12Fence,
    last: Mutex<u64>,
}

impl Queue {
    fn new(device: &ID3D12Device) -> Result<Self> {
        let device = device.cast::<ID3D12Device3>()?;
        // SAFETY: a fresh fence belonging to this D3D12 device.
        let fence = unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE)? };
        Ok(Self { device, fence, last: Mutex::new(0) })
    }

    /// Cache the paging queue, or the device's lack of support. Other setup
    /// errors can be retried on a later allocation.
    pub(super) fn get<'a>(cached: &'a OnceLock<Option<Self>>, device: &ID3D12Device) -> Result<Option<&'a Self>> {
        if let Some(queue) = cached.get() {
            return Ok(queue.as_ref());
        }
        let queue = match Self::new(device) {
            Ok(queue) => Some(queue),
            Err(e) if matches!(e.code(), E_NOINTERFACE | E_NOTIMPL | DXGI_ERROR_UNSUPPORTED) => None,
            Err(e) => return Err(e),
        };
        let supported = queue.is_some();
        if cached.set(queue).is_ok() {
            tracing::info!(supported, "device-local UAV residency fence (VICTORY_UAV_ENQUEUE_RESIDENT=1)");
        }
        Ok(cached.get().and_then(Option::as_ref))
    }

    pub(super) fn make_resident(&self, buffer: &ID3D12Resource, allocation: Allocation) -> Result<()> {
        let started = Instant::now();
        let queued = self.enqueue(buffer);
        memory_samples::record(allocation, Step::Enqueue, started.elapsed().as_micros() as u64, queued.is_err());
        let value = queued?;
        let started = Instant::now();
        let finished = self.wait(value);
        memory_samples::record(allocation, Step::WaitForFence, started.elapsed().as_micros() as u64, finished.is_err());
        finished
    }

    fn enqueue(&self, buffer: &ID3D12Resource) -> Result<u64> {
        let object = buffer.cast::<ID3D12Pageable>()?;
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let value = last.checked_add(1).filter(|&value| value != u64::MAX).ok_or_else(|| Error::from_hresult(E_FAIL))?;
        // SAFETY: the buffer and paging fence belong to this device. The
        // lock keeps paging fence values in the order the driver receives
        // them. The caller retains the buffer while waiting for its value.
        unsafe { self.device.EnqueueMakeResident(D3D12_RESIDENCY_FLAG_NONE, &[Some(object)], &self.fence, value)? };
        *last = value;
        Ok(value)
    }

    fn reached(&self, value: u64) -> Result<bool> {
        // SAFETY: a plain query on the live paging fence.
        let completed = unsafe { self.fence.GetCompletedValue() };
        if completed == u64::MAX {
            // SAFETY: a plain query on this fence's device.
            let error = unsafe { self.device.GetDeviceRemovedReason() }.err();
            return Err(error.unwrap_or_else(|| Error::from_hresult(DXGI_ERROR_DEVICE_REMOVED)));
        }
        Ok(completed >= value)
    }

    fn wait(&self, value: u64) -> Result<()> {
        if self.reached(value)? {
            return Ok(());
        }
        // SAFETY: a null event makes this call wait on the live fence. No
        // paging lock is held, so other threads can enqueue while it waits.
        if let Err(error) = unsafe { self.fence.SetEventOnCompletion(value, HANDLE::default()) } {
            tracing::warn!(%error, value, "could not register a UAV residency wait; polling the paging fence");
        }
        // If event registration failed, the successful enqueue still owns
        // this paging operation. Keep the buffer alive until it completes.
        while !self.reached(value)? {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_local_uav_can_be_used_immediately() {
        let Some(host) = crate::host::host() else { return };
        const SIZE: u64 = 4096;
        let (upload, source) = host.upload_buffer(SIZE).unwrap();
        let (back, read) = host.shared_buffer_cpu(SIZE).unwrap();
        let before = host.uav_residency.get().and_then(Option::as_ref).map_or(0, |paging| {
            *paging.last.lock().unwrap_or_else(|e| e.into_inner())
        });
        let buffer = host.uav_buffer(SIZE).unwrap();
        if !enabled() {
            assert!(host.uav_residency.get().is_none());
        } else if super::super::wait_resident()
            && let Some(paging) = host.uav_residency.get().expect("the paging mode was tried")
        {
            assert!(paging.reached(before + 1).unwrap());
        }
        // SAFETY: the three allocations have SIZE bytes; this test retains
        // the resources and command objects until the copy fence completes.
        unsafe {
            std::ptr::write_bytes(source, 0x61, SIZE as usize);
            let queue: ID3D12CommandQueue = host.device
                .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                    ..Default::default()
                })
                .unwrap();
            let allocator: ID3D12CommandAllocator = host.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                .unwrap();
            let list: ID3D12GraphicsCommandList = host.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
                .unwrap();
            list.CopyBufferRegion(&buffer, 0, &upload, 0, SIZE);
            list.ResourceBarrier(&[crate::host::transition(
                &buffer,
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            )]);
            list.CopyBufferRegion(&back, 0, &buffer, 0, SIZE);
            list.Close().unwrap();
            let fence: ID3D12Fence = host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
            queue.ExecuteCommandLists(&[Some(list.cast().unwrap())]);
            queue.Signal(&fence, 1).unwrap();
            fence.SetEventOnCompletion(1, HANDLE::default()).unwrap();
            assert_eq!(fence.GetCompletedValue(), 1);
            assert!(std::slice::from_raw_parts(read, SIZE as usize).iter().all(|&value| value == 0x61));
        }
    }

    #[test]
    fn residency_wait_requires_its_fence_value() {
        let Some(host) = crate::host::host() else { return };
        let Ok(queue) = Queue::new(&host.device) else { return };
        let (finished, done) = std::sync::mpsc::channel();
        let (started, running) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let queue = &queue;
            scope.spawn(move || {
                started.send(()).unwrap();
                finished.send(queue.wait(2)).unwrap();
            });
            running.recv().unwrap();
            // SAFETY: the test owns this fence and gives it monotonically
            // increasing values. The first cannot satisfy the wait.
            unsafe { queue.fence.Signal(1).unwrap() };
            let early = done.recv_timeout(Duration::from_millis(25));
            unsafe { queue.fence.Signal(2).unwrap() };
            assert!(matches!(early, Err(std::sync::mpsc::RecvTimeoutError::Timeout)));
            done.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
        });
    }
}
