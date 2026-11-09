//! Wait-free, lock-free Single-Producer Single-Consumer (SPSC) ring buffer for real-time audio threads.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A wait-free SPSC ring buffer for real-time audio thread communication without locks.
pub struct SpscRingBuffer<T> {
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    capacity: usize,
    head: AtomicUsize, // Written by Producer, read by Consumer
    tail: AtomicUsize, // Written by Consumer, read by Producer
}

unsafe impl<T: Send> Sync for SpscRingBuffer<T> {}
unsafe impl<T: Send> Send for SpscRingBuffer<T> {}

impl<T> SpscRingBuffer<T> {
    /// Creates a new SPSC ring buffer with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be greater than zero");
        let total_slots = capacity + 1;
        let mut buffer = Vec::with_capacity(total_slots);
        for _ in 0..total_slots {
            buffer.push(UnsafeCell::new(MaybeUninit::uninit()));
        }

        Self {
            buffer: buffer.into_boxed_slice(),
            capacity: total_slots,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Splits the ring buffer into independent Producer and Consumer handles.
    pub fn split(self) -> (Producer<T>, Consumer<T>) {
        let arc = Arc::new(self);
        (
            Producer {
                ring: arc.clone(),
            },
            Consumer {
                ring: arc,
            },
        )
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        (head + 1) % self.capacity == tail
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity - 1
    }

    #[inline]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head >= tail {
            head - tail
        } else {
            self.capacity - (tail - head)
        }
    }

    #[inline]
    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let next_head = (head + 1) % self.capacity;

        if next_head == tail {
            return Err(item); // Buffer full
        }

        unsafe {
            let slot = &mut *self.buffer[head].get();
            slot.write(item);
        }

        self.head.store(next_head, Ordering::Release);
        Ok(())
    }

    #[inline]
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if head == tail {
            return None; // Buffer empty
        }

        let item = unsafe {
            let slot = &mut *self.buffer[tail].get();
            slot.assume_init_read()
        };

        let next_tail = (tail + 1) % self.capacity;
        self.tail.store(next_tail, Ordering::Release);
        Some(item)
    }
}

impl<T> Drop for SpscRingBuffer<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

/// Producer half of the SPSC ring buffer (held by main thread).
pub struct Producer<T> {
    ring: Arc<SpscRingBuffer<T>>,
}

unsafe impl<T: Send> Send for Producer<T> {}

impl<T> Producer<T> {
    #[inline]
    pub fn push(&mut self, item: T) -> Result<(), T> {
        self.ring.push(item)
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.ring.is_full()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

/// Consumer half of the SPSC ring buffer (held by real-time audio thread).
pub struct Consumer<T> {
    ring: Arc<SpscRingBuffer<T>>,
}

unsafe impl<T: Send> Send for Consumer<T> {}

impl<T> Consumer<T> {
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.ring.pop()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.ring.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spsc_ring_buffer_fifo_order() {
        let rb = SpscRingBuffer::<i32>::new(4);
        let (mut tx, mut rx) = rb.split();

        assert!(tx.is_empty());
        assert_eq!(tx.capacity(), 4);

        assert!(tx.push(10).is_ok());
        assert!(tx.push(20).is_ok());
        assert!(tx.push(30).is_ok());
        assert!(tx.push(40).is_ok());
        assert!(tx.is_full());
        assert_eq!(tx.push(50), Err(50));

        assert_eq!(rx.pop(), Some(10));
        assert_eq!(rx.pop(), Some(20));
        assert!(!tx.is_full());

        assert!(tx.push(50).is_ok());
        assert_eq!(rx.pop(), Some(30));
        assert_eq!(rx.pop(), Some(40));
        assert_eq!(rx.pop(), Some(50));
        assert_eq!(rx.pop(), None);
    }
}
