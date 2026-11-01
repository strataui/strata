//! Fast contiguous bump memory arenas with bulk O(1) resets and SIMD sentry padding.

use std::alloc::{alloc, dealloc, Layout};
use std::cell::UnsafeCell;
use std::ptr::NonNull;

pub const DEFAULT_ARENA_CAPACITY: usize = 1024 * 1024; // 1 MB contiguous memory block
pub const SIMD_SENTRY_BYTES: usize = 32;               // 32-byte zero-tail for AVX2/NEON safety

/// Contiguous bump allocator with O(1) single-cycle bulk reset.
pub struct BumpArena {
    memory: NonNull<u8>,
    capacity: usize,
    offset: UnsafeCell<usize>,
}

unsafe impl Send for BumpArena {}
unsafe impl Sync for BumpArena {}

impl BumpArena {
    pub fn new(capacity: usize) -> Self {
        // Allocate capacity + 32-byte zero-tail sentry padding for SIMD 256-bit boundary loads
        let total_size = capacity + SIMD_SENTRY_BYTES;
        let layout = Layout::from_size_align(total_size, 64).expect("Invalid arena layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM: Failed to allocate BumpArena memory");

        // Zero the trailing SIMD sentry padding
        unsafe {
            std::ptr::write_bytes(ptr.add(capacity), 0, SIMD_SENTRY_BYTES);
        }

        Self {
            memory: NonNull::new(ptr).unwrap(),
            capacity,
            offset: UnsafeCell::new(0),
        }
    }

    /// Allocates an instance of `T` sequentially in linear memory.
    /// Panics if capacity is exceeded.
    #[inline(always)]
    pub fn alloc<T>(&self, value: T) -> &mut T {
        let layout = Layout::new::<T>();
        let current_offset = unsafe { *self.offset.get() };
        let aligned_offset = (current_offset + (layout.align() - 1)) & !(layout.align() - 1);
        let next_offset = aligned_offset + layout.size();

        assert!(
            next_offset <= self.capacity,
            "BumpArena Overflow! Allocated {} bytes exceeds capacity {}",
            next_offset,
            self.capacity
        );

        unsafe {
            *self.offset.get() = next_offset;
            let dest = self.memory.as_ptr().add(aligned_offset) as *mut T;
            std::ptr::write(dest, value);
            &mut *dest
        }
    }

    /// Allocates a contiguous slice of `T` by copying elements.
    #[inline(always)]
    pub fn alloc_slice<T: Copy>(&self, items: &[T]) -> &mut [T] {
        if items.is_empty() {
            return &mut [];
        }

        let layout = Layout::array::<T>(items.len()).expect("Array layout error");
        let current_offset = unsafe { *self.offset.get() };
        let aligned_offset = (current_offset + (layout.align() - 1)) & !(layout.align() - 1);
        let next_offset = aligned_offset + layout.size();

        assert!(next_offset <= self.capacity, "BumpArena slice overflow!");

        unsafe {
            *self.offset.get() = next_offset;
            let dest = self.memory.as_ptr().add(aligned_offset) as *mut T;
            std::ptr::copy_nonoverlapping(items.as_ptr(), dest, items.len());
            std::slice::from_raw_parts_mut(dest, items.len())
        }
    }

    /// Allocates a string slice in the arena.
    #[inline(always)]
    pub fn alloc_str(&self, text: &str) -> &str {
        let bytes = self.alloc_slice(text.as_bytes());
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    /// Single-cycle O(1) bulk reset: drops all allocations instantly by resetting the offset to 0.
    #[inline(always)]
    pub fn reset(&self) {
        unsafe {
            *self.offset.get() = 0;
        }
    }

    #[inline(always)]
    pub fn allocated_bytes(&self) -> usize {
        unsafe { *self.offset.get() }
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Drop for BumpArena {
    fn drop(&mut self) {
        let total_size = self.capacity + SIMD_SENTRY_BYTES;
        let layout = Layout::from_size_align(total_size, 64).unwrap();
        unsafe {
            dealloc(self.memory.as_ptr(), layout);
        }
    }
}

/// Double-buffered generational arena for zero-allocation UI frame reconciliation.
pub struct FrameArenaManager {
    arenas: [BumpArena; 2],
    current_index: usize,
    frame_epoch: u64,
}

impl FrameArenaManager {
    pub fn new(capacity_per_frame: usize) -> Self {
        Self {
            arenas: [
                BumpArena::new(capacity_per_frame),
                BumpArena::new(capacity_per_frame),
            ],
            current_index: 0,
            frame_epoch: 0,
        }
    }

    #[inline(always)]
    pub fn active_arena(&self) -> &BumpArena {
        &self.arenas[self.current_index]
    }

    #[inline(always)]
    pub fn previous_arena(&self) -> &BumpArena {
        &self.arenas[1 - self.current_index]
    }

    #[inline(always)]
    pub fn frame_epoch(&self) -> u64 {
        self.frame_epoch
    }

    /// Swaps the active/previous frame buffers and resets the new frame's arena in O(1).
    #[inline(always)]
    pub fn swap_and_reset(&mut self) {
        self.current_index = 1 - self.current_index;
        self.frame_epoch += 1;
        self.arenas[self.current_index].reset();
    }
}

/// Cache-line aligned (64-byte) linear arena for worker threads in parallel layout passes.
#[repr(align(64))]
pub struct ThreadLocalArena {
    start: *mut u8,
    ptr: *mut u8,
    end: *mut u8,
    capacity: usize,
}

impl ThreadLocalArena {
    pub fn new(capacity: usize) -> Self {
        let layout = Layout::from_size_align(capacity, 64).expect("Invalid layout for ThreadLocalArena");
        let start = unsafe { alloc(layout) };
        assert!(!start.is_null(), "OOM: Failed to allocate ThreadLocalArena");

        Self {
            start,
            ptr: start,
            end: unsafe { start.add(capacity) },
            capacity,
        }
    }

    #[inline(always)]
    pub fn alloc<T>(&mut self, value: T) -> &'static mut T {
        let layout = Layout::new::<T>();
        let current = self.ptr as usize;
        let aligned = (current + layout.align() - 1) & !(layout.align() - 1);
        let next = aligned + layout.size();

        assert!(next <= self.end as usize, "ThreadLocalArena overflow!");

        self.ptr = next as *mut u8;
        unsafe {
            let target = aligned as *mut T;
            target.write(value);
            &mut *target
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.ptr = self.start;
    }
}

impl Drop for ThreadLocalArena {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.capacity, 64).unwrap();
        unsafe {
            dealloc(self.start, layout);
        }
    }
}
