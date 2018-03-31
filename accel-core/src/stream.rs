/// Mock internal state of CUDA stream

use core::ptr::null_mut;
use cuda::*;

pub use cuda::cudaEventFlags as EventFlags;

pub struct Stream(cudaStream_t);

impl Stream {
    pub fn blocking() -> Self {
        let mut st = null_mut();
        unsafe { cudaStreamCreateWithFlags(&mut st as *mut _, cudaStreamFlags::Default) }.check();
        Stream(st)
    }

    pub fn non_blocking() -> Self {
        let mut st = null_mut();
        unsafe { cudaStreamCreateWithFlags(&mut st as *mut _, cudaStreamFlags::NonBlocking) }.check();
        Stream(st)
    }

    pub fn wait(&self, event: &Event) {
        unsafe { cudaStreamWaitEvent(self.0, event.0, 0) }.check();
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe { cudaStreamDestroy(self.0) }.check();
    }
}

pub struct Event(cudaEvent_t);

impl Event {
    pub fn new(flags: EventFlags) -> Self {
        let mut st = null_mut();
        unsafe { cudaEventCreateWithFlags(&mut st as *mut _, flags) }.check();
        Event(st)
    }

    pub fn record(&mut self, stream: &Stream) {
        unsafe { cudaEventRecord(self.0, stream.0) }.check();
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe { cudaEventDestroy(self.0) }.check();
    }
}
