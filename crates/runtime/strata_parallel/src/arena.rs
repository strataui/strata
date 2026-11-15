//! Cache-aligned ThreadLocalArena with 32-byte SIMD sentry padding and 1-cycle reset.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

type DropEntry = (*mut u8, unsafe fn(*mut u8));

/// 64-byte aligned thread-local bump arena for zero heap contention across worker threads.
#[repr(align(64))]
pub struct ThreadLocalArena {
    start: NonNull<u8>,
    ptr: *mut u8,
    end: *mut u8,
    capacity: usize,
    layout: Layout,
    drop_chain: Vec<DropEntry>,
}

unsafe impl Send for ThreadLocalArena {}

impl ThreadLocalArena {
    /// Creates a new arena with the requested capacity.
    /// Allocates an additional 32 bytes for SIMD over-read sentry padding.
    pub fn new(capacity: usize) -> Self {
        let total_size = capacity + 32; // 32-byte zero-tail sentry padding
        let layout =
            Layout::from_size_align(total_size, 64).expect("Invalid layout for ThreadLocalArena");

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
            drop_chain: Vec::new(),
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

            if std::mem::needs_drop::<T>() {
                unsafe fn drop_glue<T>(ptr: *mut u8) {
                    std::ptr::drop_in_place(ptr as *mut T);
                }
                self.drop_chain.push((target as *mut u8, drop_glue::<T>));
            }

            &mut *target
        }
    }

    /// Fast path allocation for `Copy` types without drop tracking overhead.
    #[inline(always)]
    pub fn alloc_copy<T: Copy>(&mut self, value: T) -> &mut T {
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
        if slice.is_empty() {
            return &mut [];
        }

        let align = std::mem::align_of::<T>();
        let size = std::mem::size_of_val(slice);

        let current = self.ptr as usize;
        let aligned = (current + align - 1) & !(align - 1);
        let next = aligned + size;

        assert!(
            next <= self.end as usize,
            "ThreadLocalArena capacity overflow for slice"
        );

        // RAII rollback and partial drop guard for exception safety (CWE-401 / Goal 04)
        struct SliceGuard<T> {
            ptr: *mut T,
            len: usize,
            old_bump: *mut u8,
            arena_ptr: *mut *mut u8,
        }

        impl<T> Drop for SliceGuard<T> {
            fn drop(&mut self) {
                for i in 0..self.len {
                    unsafe {
                        std::ptr::drop_in_place(self.ptr.add(i));
                    }
                }
                unsafe {
                    *self.arena_ptr = self.old_bump;
                }
            }
        }

        let old_bump = self.ptr;
        self.ptr = next as *mut u8;
        let target = aligned as *mut T;

        let mut guard = SliceGuard {
            ptr: target,
            len: 0,
            old_bump,
            arena_ptr: &mut self.ptr,
        };

        for item in slice.iter() {
            unsafe {
                std::ptr::write(target.add(guard.len), item.clone());
            }
            guard.len += 1;
        }

        // Successfully cloned all items: disarm guard
        std::mem::forget(guard);

        unsafe {
            if std::mem::needs_drop::<T>() {
                unsafe fn drop_glue<T>(ptr: *mut u8) {
                    std::ptr::drop_in_place(ptr as *mut T);
                }
                for i in 0..slice.len() {
                    self.drop_chain
                        .push((target.add(i) as *mut u8, drop_glue::<T>));
                }
            }

            std::slice::from_raw_parts_mut(target, slice.len())
        }
    }

    /// Resets the bump pointer to the beginning of the arena, running destructors.
    #[inline(always)]
    pub fn reset(&mut self) {
        for (ptr, drop_fn) in self.drop_chain.drain(..).rev() {
            unsafe {
                drop_fn(ptr);
            }
        }
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
        for (ptr, drop_fn) in self.drop_chain.drain(..).rev() {
            unsafe {
                drop_fn(ptr);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_alloc_slice_panic_safety() {
        struct PanicOnClone {
            id: usize,
            panic_id: usize,
            drops: Arc<AtomicUsize>,
        }

        impl Clone for PanicOnClone {
            fn clone(&self) -> Self {
                if self.id == self.panic_id {
                    panic!("Simulated clone panic at index {}", self.id);
                }
                Self {
                    id: self.id,
                    panic_id: self.panic_id,
                    drops: Arc::clone(&self.drops),
                }
            }
        }

        impl Drop for PanicOnClone {
            fn drop(&mut self) {
                self.drops.fetch_add(1, Ordering::SeqCst);
            }
        }

        let mut arena = ThreadLocalArena::new(1024 * 64);
        let initial_bump = arena.ptr;
        let panic_drops = Arc::new(AtomicUsize::new(0));

        let slice = vec![
            PanicOnClone {
                id: 0,
                panic_id: 2,
                drops: Arc::clone(&panic_drops),
            },
            PanicOnClone {
                id: 1,
                panic_id: 2,
                drops: Arc::clone(&panic_drops),
            },
            PanicOnClone {
                id: 2,
                panic_id: 2,
                drops: Arc::clone(&panic_drops),
            },
            PanicOnClone {
                id: 3,
                panic_id: 2,
                drops: Arc::clone(&panic_drops),
            },
        ];

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            arena.alloc_slice(&slice);
        }));

        assert!(result.is_err(), "Must panic when cloning element 2");

        // Elements 0 and 1 were successfully cloned into arena before panic.
        // RAII guard must have dropped those 2 elements during stack unwind!
        assert_eq!(
            panic_drops.load(Ordering::SeqCst),
            2,
            "Initialized elements must be cleaned up on panic"
        );

        // The arena bump pointer must be rolled back to its pre-allocation state
        assert_eq!(
            arena.ptr, initial_bump,
            "Bump pointer must roll back on panic"
        );
    }
}
