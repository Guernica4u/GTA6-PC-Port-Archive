use super::*;

#[test]
#[ignore = "requires D3D12 WARP; run in an isolated test process"]
fn async_prewarm_preserves_an_in_flight_read() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    const OLD: u32 = 0x11223344;
    const NEW: u32 = 0xaabbccdd;

    fn run(asynchronous: bool, overlap: bool) -> [u32; 2] {
        let mut host = Host::new_warp().expect("WARP device required");
        let (release, receiver) = mpsc::channel();
        if asynchronous {
            host.test_release_before_drain = Some(release);
        }

        let mut words = Box::new([OLD, OLD, OLD]);
        let address = words.as_mut_ptr() as u64;
        let second_offset = if overlap { 0 } else { 4 };
        let (buffer, mapped) = host.upload_buffer(12).unwrap();
        let shadow = crate::shadow::Shadow::test_upload(buffer, mapped, address, 12);
        let (output, bytes) = host.readback_buffer(8).unwrap();
        let record = |source_offset: u64, output_offset, refresh_bytes| {
            let list = Arc::new(host.new_list().unwrap());
            shadow.refresh(&list, address + source_offset, refresh_bytes);
            // SAFETY: upload and readback heaps support these copy states.
            // Both resources outlive the two command lists and the fence.
            unsafe {
                list.list.CopyBufferRegion(
                    &output,
                    output_offset,
                    &shadow.buffer,
                    source_offset,
                    4,
                );
            }
            list.close();
            list
        };
        let first = record(0, 0, 4);
        // The eight-byte refresh either covers or directly follows the first four-byte read.
        let second = record(second_offset, 4, 8);
        std::thread::scope(|scope| {
            let watchdog = if asynchronous {
                // SAFETY: the queue waits on an independent CPU-signalled fence.
                let gate: ID3D12Fence =
                    unsafe { host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.unwrap();
                // SAFETY: the queue and gate belong to the same live device.
                unsafe { host.queue.Wait(&gate, 1) }.unwrap();
                Some(scope.spawn(move || {
                    let released_by_submission =
                        receiver.recv_timeout(Duration::from_secs(10)).is_ok();
                    // SAFETY: this thread owns the live gate and signals it even on timeout.
                    unsafe { gate.Signal(1) }.unwrap();
                    released_by_submission
                }))
            } else {
                None
            };
            host.execute_title(&[first]);
            assert_eq!(host.has_in_flight(), asynchronous);
            words[(second_offset / 4) as usize] = NEW;
            host.execute_title(&[second]);
            // SAFETY: signal a fresh fence on its own device queue after both copies.
            // Verify completion here because fence values are local to this host.
            unsafe {
                let done: ID3D12Fence = host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
                host.queue.Signal(&done, 1).unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                while done.GetCompletedValue() == 0 && std::time::Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert_eq!(
                    done.GetCompletedValue(),
                    1,
                    "isolated queue did not complete"
                );
            }
            host.reap_completed();
            if let Some(thread) = watchdog {
                assert!(
                    thread.join().unwrap(),
                    "second submission missed fence checkpoint"
                );
            }
        });
        // SAFETY: both four-byte copies completed. The readback stays mapped.
        unsafe {
            [
                bytes.cast::<u32>().read_unaligned(),
                bytes.add(4).cast::<u32>().read_unaligned(),
            ]
        }
    }

    assert_eq!(run(false, true), [OLD, NEW], "synchronous overlap control");
    assert_eq!(
        run(true, false),
        [OLD, NEW],
        "asynchronous disjoint control"
    );
    assert_eq!(
        run(true, true),
        [OLD, NEW],
        "asynchronous overlapping refresh"
    );
}

#[test]
#[ignore = "requires D3D12 WARP; run in an isolated test process"]
fn async_prewarm_coalesced_refresh_preserves_an_in_flight_read() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    const OLD: u32 = 0x11223344;
    const NEW: u32 = 0xaabbccdd;

    // Offsets are relative to the upload buffer. Write-watch pages use
    // absolute addresses, and the backing allocation need not be aligned.
    // Individual four-byte refreshes exclude READ_OFFSET; their merged
    // interval includes it. Page filtering still clips to that interval.
    const PAGE: u64 = 64 << 10;
    const READ_OFFSET: u64 = PAGE + 512;
    const HIGH_OFFSET: u64 = 2 * PAGE;
    const LEN: u64 = 2 * PAGE + 4096;

    // Returns [the in-flight read at READ_OFFSET, a read of offset 0 made
    // by the second submission]. Neither source word at offset 0 or
    // HIGH_OFFSET ever changes, so the correct result is [OLD, OLD] in
    // both variants. Only the CPU source at READ_OFFSET changes, to NEW,
    // while the first submission is in flight.
    fn run(two_refreshes: bool) -> [u32; 2] {
        let mut host = Host::new_warp().expect("WARP device required");
        let (release, receiver) = mpsc::channel();
        host.test_release_before_drain = Some(release);

        let mut words = vec![OLD; (LEN / 4) as usize].into_boxed_slice();
        let address = words.as_mut_ptr() as u64;
        let (buffer, mapped) = host.upload_buffer(LEN).unwrap();
        let shadow = crate::shadow::Shadow::test_upload(buffer, mapped, address, LEN);
        let (output, bytes) = host.readback_buffer(8).unwrap();
        let record = |source_offset: u64, output_offset: u64, refresh_offsets: &[u64]| {
            let list = Arc::new(host.new_list().unwrap());
            for &refresh_offset in refresh_offsets {
                shadow.refresh(&list, address + refresh_offset, 4);
            }
            // SAFETY: upload and readback heaps support these copy states.
            // Both resources outlive the two command lists and the fence.
            unsafe {
                list.list.CopyBufferRegion(
                    &output,
                    output_offset,
                    &shadow.buffer,
                    source_offset,
                    4,
                );
            }
            list.close();
            list
        };
        let first = record(READ_OFFSET, 0, &[READ_OFFSET]);
        // The second list refreshes only offset 0 in the control, and
        // offsets 0 and HIGH_OFFSET in the experiment. Neither range
        // overlaps READ_OFFSET; a batch that merges them into the range
        // [0, HIGH_OFFSET + 4) spans it. Its own copy reads offset 0.
        let second_refreshes: &[u64] = if two_refreshes {
            &[0, HIGH_OFFSET]
        } else {
            &[0]
        };
        let second = record(0, 4, second_refreshes);
        std::thread::scope(|scope| {
            // SAFETY: the queue waits on an independent CPU-signalled fence.
            let gate: ID3D12Fence =
                unsafe { host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.unwrap();
            // SAFETY: the queue and gate belong to the same live device.
            unsafe { host.queue.Wait(&gate, 1) }.unwrap();
            let watchdog = scope.spawn(move || {
                let released_by_submission = receiver.recv_timeout(Duration::from_secs(10)).is_ok();
                // SAFETY: this thread owns the live gate and signals it even on timeout.
                unsafe { gate.Signal(1) }.unwrap();
                released_by_submission
            });
            host.execute_title(&[first]);
            assert!(host.has_in_flight(), "first submission must stay in flight");
            words[(READ_OFFSET / 4) as usize] = NEW;
            host.execute_title(&[second]);
            // SAFETY: signal a fresh fence on its own device queue after both copies.
            // Verify completion here because fence values are local to this host.
            unsafe {
                let done: ID3D12Fence = host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
                host.queue.Signal(&done, 1).unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                while done.GetCompletedValue() == 0 && std::time::Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert_eq!(
                    done.GetCompletedValue(),
                    1,
                    "isolated queue did not complete"
                );
            }
            host.reap_completed();
            assert!(
                watchdog.join().unwrap(),
                "second submission missed fence checkpoint"
            );
        });
        // SAFETY: both four-byte copies completed. The readback stays mapped.
        unsafe {
            [
                bytes.cast::<u32>().read_unaligned(),
                bytes.add(4).cast::<u32>().read_unaligned(),
            ]
        }
    }

    assert_eq!(run(false), [OLD, OLD], "single disjoint refresh control");
    assert_eq!(
        run(true),
        [OLD, OLD],
        "two disjoint refreshes must not overwrite the in-flight read between them"
    );
}
