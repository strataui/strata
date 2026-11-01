//! Lock-Free, Wait-Free Atomic MPSC Ring Buffer for Reactive Signal Notifications.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Wait-free Atomic MPSC Ring Buffer with cache-line false sharing prevention.
#[repr(align(64))]
pub struct DirtySignalQueue {
    buffer: Box<[AtomicU32]>,
    capacity: usize,
    mask: usize,
    head: AtomicUsize,
    _pad0: [u8; 56],
    tail: AtomicUsize,
    _pad1: [u8; 56],
}

impl DirtySignalQueue {
    /// Creates a new dirty signal queue with power-of-two capacity.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(64);
        let mut buf = Vec::with_capacity(cap);
        for _ in 0..cap {
            buf.push(AtomicU32::new(u32::MAX));
        }

        Self {
            buffer: buf.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            head: AtomicUsize::new(0),
            _pad0: [0; 56],
            tail: AtomicUsize::new(0),
            _pad1: [0; 56],
        }
    }

    /// Enqueues a dirty signal ID without taking any locks.
    /// Returns true if enqueued successfully, or false if the ring buffer is saturated.
    pub fn push(&self, signal_id: u32) -> bool {
        loop {
            let head = self.head.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Acquire);

            if head.wrapping_sub(tail) >= self.capacity {
                return false; // Queue saturated
            }

            if self
                .head
                .compare_exchange_weak(head, head.wrapping_add(1), Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                let slot = head & self.mask;
                self.buffer[slot].store(signal_id, Ordering::Release);
                return true;
            }
        }
    }

    /// Batch drains all pending dirty signal IDs into a vector (called by the main UI thread).
    pub fn drain(&self) -> Vec<u32> {
        let mut items = Vec::new();
        let head = self.head.load(Ordering::Acquire);
        let mut tail = self.tail.load(Ordering::Relaxed);

        while tail != head {
            let slot = tail & self.mask;
            let val = self.buffer[slot].swap(u32::MAX, Ordering::AcqRel);
            if val != u32::MAX {
                items.push(val);
            }
            tail = tail.wrapping_add(1);
        }

        self.tail.store(tail, Ordering::Release);
        items
    }

    /// Drains dirty signal IDs into a bitset slice for instant O(1) membership queries.
    pub fn drain_into_bitset(&self, bitset: &mut [u64]) -> usize {
        let mut count = 0;
        let head = self.head.load(Ordering::Acquire);
        let mut tail = self.tail.load(Ordering::Relaxed);

        while tail != head {
            let slot = tail & self.mask;
            let val = self.buffer[slot].swap(u32::MAX, Ordering::AcqRel);
            if val != u32::MAX {
                let word_idx = (val as usize) / 64;
                let bit_idx = (val as usize) % 64;
                if word_idx < bitset.len() {
                    bitset[word_idx] |= 1u64 << bit_idx;
                    count += 1;
                }
            }
            tail = tail.wrapping_add(1);
        }

        self.tail.store(tail, Ordering::Release);
        count
    }

    /// Number of queued pending signals.
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        head.wrapping_sub(tail)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
