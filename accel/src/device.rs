//! CUDA [Device] and [Context]
//!
//! [Device]:  https://docs.nvidia.com/cuda/cuda-driver-api/group__CUDA__DEVICE.html
//! [Context]: https://docs.nvidia.com/cuda/cuda-driver-api/group__CUDA__CTX.html

use crate::{error::*, *};
use cuda::*;
use std::sync::{Arc, Once};

/// Handler for device and its primary context
#[derive(Debug, PartialEq, PartialOrd)]
pub struct Device {
    device: CUdevice,
}

impl Device {
    /// Initializer for CUDA Driver API
    fn init() {
        static DRIVER_API_INIT: Once = Once::new();
        DRIVER_API_INIT.call_once(|| unsafe {
            ffi_call!(cuda::cuInit, 0).expect("Initialization of CUDA Driver API failed");
        });
    }

    /// Get number of available GPUs
    pub fn get_count() -> Result<usize> {
        Self::init();
        let mut count: i32 = 0;
        unsafe {
            ffi_call!(cuDeviceGetCount, &mut count as *mut i32)?;
        }
        Ok(count as usize)
    }

    pub fn nth(id: usize) -> Result<Self> {
        let count = Self::get_count()?;
        if id >= count {
            return Err(AccelError::DeviceNotFound { id, count });
        }
        let device = unsafe { ffi_new!(cuDeviceGet, id as i32)? };
        Ok(Device { device })
    }

    /// Get total memory of GPU
    pub fn total_memory(&self) -> Result<usize> {
        let mut mem = 0;
        unsafe {
            ffi_call!(cuDeviceTotalMem_v2, &mut mem as *mut _, self.device)?;
        }
        Ok(mem)
    }

    /// Get name of GPU
    pub fn get_name(&self) -> Result<String> {
        let mut bytes: Vec<u8> = vec![0_u8; 1024];
        unsafe {
            ffi_call!(
                cuDeviceGetName,
                bytes.as_mut_ptr() as *mut i8,
                1024,
                self.device
            )?;
        }
        Ok(String::from_utf8(bytes).expect("GPU name is not UTF8"))
    }

    /// Create a new CUDA context on this device.
    ///
    /// ```
    /// # use accel::*;
    /// let device = Device::nth(0).unwrap();
    /// let ctx = device.create_context();
    /// ```
    pub fn create_context(&self) -> Arc<Context> {
        Arc::new(Context::create(self.device))
    }
}

/// RAII handler for using CUDA context
///
/// As described in [CUDA Programming Guide], library using CUDA should push context before using
/// it, and then pop it.
///
/// [CUDA Programming Guide]: https://docs.nvidia.com/cuda/cuda-c-programming-guide/index.html#context
pub struct ContextGuard {
    ctx: Arc<Context>,
}

impl ContextGuard {
    /// Make context as current on this thread
    pub fn guard_context(ctx: Arc<Context>) -> Self {
        ctx.push();
        Self { ctx }
    }
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        self.ctx.pop();
    }
}

/// Object tied up to a CUDA context
pub trait Contexted {
    /// Create a dynamically managed, counted reference to Context
    ///
    /// See https://gitlab.com/termoshtt/accel/-/merge_requests/66
    fn get_context(&self) -> Arc<Context>;

    /// RAII utility for push/pop onto the thread-local context stack
    fn guard_context(&self) -> ContextGuard {
        let ctx = self.get_context();
        ContextGuard::guard_context(ctx)
    }

    /// Blocking until all tasks in current context end
    fn sync_context(&self) -> Result<()> {
        let ctx = self.get_context();
        ctx.sync()?;
        Ok(())
    }
}

/// Owend handler for CUDA context
#[derive(Debug, PartialEq)]
pub struct Context {
    ptr: CUcontext,
}

impl Drop for Context {
    fn drop(&mut self) {
        if let Err(e) = unsafe { ffi_call!(cuCtxDestroy_v2, self.ptr) } {
            log::error!("Context remove failed: {:?}", e);
        }
    }
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

/// Non-Owend handler for CUDA context
///
/// The validity of reference is checked dynamically.
/// CUDA APIs (e.g. [cuPointerGetAttribute]) allow us to get a pointer to CUDA context,
/// but its validity cannot be assured by Rust lifetime system.
///
/// [cuPointerGetAttribute]: https://docs.nvidia.com/cuda/cuda-driver-api/group__CUDA__UNIFIED.html#group__CUDA__UNIFIED_1g0c28ed0aff848042bc0533110e45820c
///
#[derive(Debug, PartialEq)]
pub struct ContextRef {
    ptr: CUcontext,
}

impl std::cmp::PartialEq<ContextRef> for Context {
    fn eq(&self, ctx: &ContextRef) -> bool {
        self.ptr == ctx.ptr
    }
}

impl std::cmp::PartialEq<Context> for ContextRef {
    fn eq(&self, ctx: &Context) -> bool {
        self.ptr == ctx.ptr
    }
}

unsafe impl Send for ContextRef {}
unsafe impl Sync for ContextRef {}

/// Common implementations for Context, ContextRef
pub(crate) trait ContextImpl {
    fn get_ptr(&self) -> CUcontext;

    /// Push to the context stack of this thread
    fn push(&self) {
        unsafe {
            ffi_call!(cuCtxPushCurrent_v2, self.get_ptr()).expect("Failed to push context");
        }
    }

    /// Pop from the context stack of this thread
    fn pop(&self) {
        let ptr = unsafe { ffi_new!(cuCtxPopCurrent_v2).expect("Failed to pop current context") };
        if ptr.is_null() {
            panic!("No current context");
        }
        assert!(ptr == self.get_ptr(), "Pop must return same pointer");
    }

    /// Get API version
    fn version(&self) -> u32 {
        let mut version: u32 = 0;
        unsafe { ffi_call!(cuCtxGetApiVersion, self.get_ptr(), &mut version as *mut _) }
            .expect("Failed to get Driver API version");
        version
    }

    /// Block until all tasks in this context to be complete.
    fn sync(&self) -> Result<()> {
        self.push();
        unsafe {
            ffi_call!(cuCtxSynchronize)?;
        }
        self.pop();
        Ok(())
    }
}

impl ContextImpl for Context {
    fn get_ptr(&self) -> CUcontext {
        self.ptr
    }
}

impl ContextImpl for ContextRef {
    fn get_ptr(&self) -> CUcontext {
        // FIXME Check pointer is still valid
        self.ptr
    }
}

impl Context {
    /// Create on the top of context stack
    fn create(device: CUdevice) -> Self {
        let ptr = unsafe {
            ffi_new!(
                cuCtxCreate_v2,
                CUctx_flags_enum::CU_CTX_SCHED_AUTO as u32,
                device
            )
        }
        .expect("Failed to create a new context");
        if ptr.is_null() {
            panic!("Cannot crate a new context");
        }
        let ctx = Context { ptr };
        ctx.pop();
        ctx
    }

    /// Get a reference
    ///
    /// This is **NOT** a Rust reference, i.e. you can drop owned context while the reference exists.
    /// The reference becomes expired after owned context is released, and it will cause a runtime error.
    ///
    pub fn get_ref(&self) -> ContextRef {
        ContextRef { ptr: self.ptr }
    }
}

impl Contexted for Arc<Context> {
    fn get_context(&self) -> Arc<Context> {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_count() -> Result<()> {
        Device::get_count()?;
        Ok(())
    }

    #[test]
    fn get_zeroth() -> Result<()> {
        Device::nth(0)?;
        Ok(())
    }

    #[test]
    fn out_of_range() -> Result<()> {
        assert!(Device::nth(129).is_err());
        Ok(())
    }

    #[test]
    fn create() -> Result<()> {
        let device = Device::nth(0)?;
        let ctx = device.create_context();
        dbg!(&ctx);
        Ok(())
    }

    #[should_panic]
    #[test]
    fn expired_context_ref() {
        let device = Device::nth(0).unwrap();
        let ctx = device.create_context();
        let ctx_ref = ctx.get_ref();
        drop(ctx);
        let _version = ctx_ref.version(); // ctx has been expired
    }
}
