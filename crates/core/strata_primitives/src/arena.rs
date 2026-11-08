//! Fast contiguous bump memory arenas with bulk O(1) resets and SIMD sentry padding.

use std::alloc::{alloc, dealloc, Layout};
use std::cell::UnsafeCell;
use std::ptr::NonNull;

pub const DEFAULT_ARENA_CAPACITY: usize = 1024 * 1024; // 1 MB contiguous memory block
pub const SIMD_SENTRY_BYTES: usize = 32;               // 32-byte zero-tail for AVX2/NEON safety

/// Type-erased destructor cleanup entry.
type DropFn = unsafe fn(*mut u8);

/// Contiguous bump allocator with O(1) single-cycle bulk reset.
pub struct BumpArena {
    memory: NonNull<u8>,
    capacity: usize,
    offset: UnsafeCell<usize>,
    drop_chain: UnsafeCell<Vec<(*mut u8, DropFn)>>,
}

unsafe impl Send for BumpArena {}
// NOTE: BumpArena is intentionally !Sync because allocation mutates internal offset
// without atomic synchronization. For multi-threaded sharing, each worker thread must
// own its dedicated arena.

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
            drop_chain: UnsafeCell::new(Vec::new()),
        }
    }

    /// Allocates an instance of `T` sequentially in linear memory.
    /// The returned reference's lifetime is bound to `&'a self`.
    /// Panics if capacity is exceeded.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
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

            if std::mem::needs_drop::<T>() {
                unsafe fn drop_glue<T>(ptr: *mut u8) {
                    std::ptr::drop_in_place(ptr as *mut T);
                }
                (*self.drop_chain.get()).push((dest as *mut u8, drop_glue::<T>));
            }

            &mut *dest
        }
    }

    /// Allocates a contiguous slice of `T` by copying elements.
    /// The returned slice's lifetime is bound to `&'a self`.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_slice<'a, T: Copy>(&'a self, items: &[T]) -> &'a mut [T] {
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
    pub fn alloc_str<'a>(&'a self, text: &str) -> &'a str {
        let bytes = self.alloc_slice(text.as_bytes());
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    /// Single-cycle O(1) bulk reset: executes destructors for all non-Copy allocations
    /// and resets the offset to 0.
    /// Requires an exclusive mutable borrow (&mut self) to guarantee at compile time that
    /// no live references to arena memory exist.
    #[inline(always)]
    pub fn reset(&mut self) {
        unsafe {
            let drops = &mut *self.drop_chain.get();
            for (ptr, drop_fn) in drops.drain(..).rev() {
                drop_fn(ptr);
            }
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
        unsafe {
            let drops = &mut *self.drop_chain.get();
            for (ptr, drop_fn) in drops.drain(..).rev() {
                drop_fn(ptr);
            }
        }
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
    pub fn alloc<T>(&mut self, value: T) -> &mut T {
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
    pub fn alloc_slice<'a, T: Copy>(&'a mut self, items: &[T]) -> &'a mut [T] {
        if items.is_empty() {
            return &mut [];
        }

        let layout = Layout::array::<T>(items.len()).expect("Array layout error");
        let current = self.ptr as usize;
        let aligned = (current + layout.align() - 1) & !(layout.align() - 1);
        let next = aligned + layout.size();

        assert!(next <= self.end as usize, "ThreadLocalArena slice overflow!");

        self.ptr = next as *mut u8;
        unsafe {
            let target = aligned as *mut T;
            std::ptr::copy_nonoverlapping(items.as_ptr(), target, items.len());
            std::slice::from_raw_parts_mut(target, items.len())
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.ptr = self.start;
    }

    #[inline(always)]
    pub fn bytes_allocated(&self) -> usize {
        (self.ptr as usize).saturating_sub(self.start as usize)
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
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
