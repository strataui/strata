//! Cache-aligned ThreadLocalArena with 32-byte SIMD sentry padding and 1-cycle reset.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

/// 64-byte aligned thread-local bump arena for zero heap contention across worker threads.
#[repr(align(64))]
pub struct ThreadLocalArena {
    start: NonNull<u8>,
    ptr: *mut u8,
    end: *mut u8,
    capacity: usize,
    layout: Layout,
}

unsafe impl Send for ThreadLocalArena {}

impl ThreadLocalArena {
    /// Creates a new arena with the requested capacity.
    /// Allocates an additional 32 bytes for SIMD over-read sentry padding.
    pub fn new(capacity: usize) -> Self {
        let total_size = capacity + 32; // 32-byte zero-tail sentry padding
        let layout = Layout::from_size_align(total_size, 64)
            .expect("Invalid layout for ThreadLocalArena");

        let raw = unsafe { alloc_zeroed(layout) };
        let start = NonNull::new(raw).expect("ThreadLocalArena allocation failed");
        let ptr = start.as_ptr();
        let end = unsafe { ptr.add(capacity) };

        Self {
            start,
            ptr,
            end,
            capacity,
            layout,
        }
    }

    /// Allocates space for an item of type `T` and moves `value` into it.
    #[inline(always)]
    pub fn alloc<T>(&mut self, value: T) -> &mut T {
        let align = std::mem::align_of::<T>();
        let size = std::mem::size_of::<T>();

        let current = self.ptr as usize;
        let aligned = (current + align - 1) & !(align - 1);
        let next = aligned + size;

        assert!(
            next <= self.end as usize,
            "ThreadLocalArena capacity overflow (requested {}, remaining {})",
            size,
            (self.end as usize).saturating_sub(aligned)
        );

        self.ptr = next as *mut u8;

        unsafe {
            let target = aligned as *mut T;
            std::ptr::write(target, value);
            &mut *target
        }
    }

    /// Allocates a contiguous slice of cloned elements.
    pub fn alloc_slice<T: Clone>(&mut self, slice: &[T]) -> &mut [T] {
        let align = std::mem::align_of::<T>();
        let size = std::mem::size_of_val(slice);

        let current = self.ptr as usize;
        let aligned = (current + align - 1) & !(align - 1);
        let next = aligned + size;

        assert!(
            next <= self.end as usize,
            "ThreadLocalArena capacity overflow for slice"
        );

        self.ptr = next as *mut u8;

        unsafe {
            let target = aligned as *mut T;
            for (i, item) in slice.iter().enumerate() {
                std::ptr::write(target.add(i), item.clone());
            }
            std::slice::from_raw_parts_mut(target, slice.len())
        }
    }

    /// Resets the bump pointer to the beginning of the arena in 1 CPU cycle.
    #[inline(always)]
    pub fn reset(&mut self) {
        self.ptr = self.start.as_ptr();
    }

    /// Total capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Current allocated bytes in the arena.
    pub fn bytes_allocated(&self) -> usize {
        (self.ptr as usize) - (self.start.as_ptr() as usize)
    }
}

impl Drop for ThreadLocalArena {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.start.as_ptr(), self.layout);
        }
    }
}

thread_local! {
    static PER_THREAD_ARENA: std::cell::RefCell<ThreadLocalArena> = std::cell::RefCell::new(ThreadLocalArena::new(1024 * 512));
}

/// Executes a closure with access to the persistent thread-local bump arena.
pub fn with_thread_local_arena<R>(f: impl FnOnce(&mut ThreadLocalArena) -> R) -> R {
    PER_THREAD_ARENA.with(|a| {
        let mut arena = a.borrow_mut();
        f(&mut arena)
    })
}

/// Resets the current thread's thread-local bump arena in 1 CPU cycle.
pub fn reset_thread_local_arena() {
    PER_THREAD_ARENA.with(|a| {
        a.borrow_mut().reset();
    });
}

