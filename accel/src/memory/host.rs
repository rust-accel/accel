//! Device and Host memory handlers

use super::CudaMemory;
use crate::{device::*, ffi_call, ffi_new};
use cuda::*;
use std::ops::{Deref, DerefMut};

/// Memory allocated as page-locked
pub struct PageLockedMemory<'ctx, T> {
    ptr: *mut T,
    size: usize,
    context: &'ctx Context,
}

impl<'ctx, T> Drop for PageLockedMemory<'ctx, T> {
    fn drop(&mut self) {
        if let Err(e) = ffi_call!(cuMemFreeHost, self.ptr as *mut _) {
            log::error!("Cannot free page-locked memory: {:?}", e);
        }
    }
}

impl<'ctx, T> Deref for PageLockedMemory<'ctx, T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<'ctx, T> DerefMut for PageLockedMemory<'ctx, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<'ctx, T> Contexted for PageLockedMemory<'ctx, T> {
    fn get_context(&self) -> &Context {
        &self.context
    }
}

impl<'ctx, T> CudaMemory<T> for PageLockedMemory<'ctx, T> {
    fn as_ptr(&self) -> *const T {
        self.ptr as *const T
    }

    fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    fn len(&self) -> usize {
        self.size
    }
}

impl<'ctx, T> PageLockedMemory<'ctx, T> {
    /// Allocate host memory as page-locked.
    ///
    /// Allocating excessive amounts of pinned memory may degrade system performance,
    /// since it reduces the amount of memory available to the system for paging.
    /// As a result, this function is best used sparingly to allocate staging areas for data exchange between host and device.
    ///
    /// See also [cuMemAllocHost].
    ///
    /// [cuMemAllocHost]: https://docs.nvidia.com/cuda/cuda-driver-api/group__CUDA__MEM.html#group__CUDA__MEM_1gdd8311286d2c2691605362c689bc64e0
    ///
    /// Panic
    /// ------
    /// - when memory allocation failed includeing `size == 0` case
    ///
    pub fn new(context: &'ctx Context, size: usize) -> Self {
        assert!(size > 0, "Zero-sized malloc is forbidden");
        let _g = context.guard_context();
        let ptr = ffi_new!(cuMemAllocHost_v2, size * std::mem::size_of::<T>())
            .expect("Cannot allocate page-locked memory");
        Self {
            ptr: ptr as *mut T,
            size,
            context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::*;

    #[test]
    fn page_locked() -> Result<()> {
        let device = Device::nth(0)?;
        let ctx = device.create_context();
        let mut mem = PageLockedMemory::<i32>::new(&ctx, 12);
        assert_eq!(mem.len(), 12);
        assert_eq!(mem.byte_size(), 12 * 4 /* size of i32 */);
        let sl = mem.as_mut_slice();
        sl[0] = 3;
        Ok(())
    }

    #[should_panic(expected = "Zero-sized malloc is forbidden")]
    #[test]
    fn page_locked_new_zero() {
        let device = Device::nth(0).unwrap();
        let ctx = device.create_context();
        let _a = PageLockedMemory::<i32>::new(&ctx, 0);
    }
}
