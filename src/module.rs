
use ffi::cuda::*;
use ffi::vector_types::*;
use error::*;

use std::ptr::null_mut;
use std::os::raw::{c_char, c_void};
use std::str::FromStr;

#[derive(Debug)]
pub struct Module(CUmodule);

impl Module {
    pub fn load(filename: &str) -> Result<Self> {
        let filename = str2cstring(filename);
        let mut handle = null_mut();
        unsafe { cuModuleLoad(&mut handle as *mut CUmodule, filename.as_ptr()) }
            .check()?;
        Ok(Module(handle))
    }

    pub fn get_function<'m>(&'m self, name: &str) -> Result<Function<'m>> {
        let name = str2cstring(name);
        let mut func = null_mut();
        unsafe { cuModuleGetFunction(&mut func as *mut CUfunction, self.0, name.as_ptr()) }
            .check()?;
        Ok(Function { func, _m: self })
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        unsafe { cuModuleUnload(self.0) }.check().expect(
            "Failed to unload module",
        );
    }
}

#[derive(Debug)]
pub struct Function<'m> {
    func: CUfunction,
    _m: &'m Module,
}

impl<'m> Function<'m> {
    pub unsafe fn launch(
        &mut self,
        args: *mut *mut c_void,
        grid: Grid,
        block: Block,
    ) -> Result<()> {
        cuLaunchKernel(
            self.func,
            grid.x,
            grid.y,
            grid.z,
            block.x,
            block.y,
            block.z,
            0, // FIXME: no shared memory
            null_mut(), // use default stream
            args,
            null_mut(), // no extra
        ).check()
    }
}

#[derive(Debug, Clone, Copy, NewType)]
pub struct Dim3(dim3);

impl Dim3 {
    pub fn x(x: u32) -> Self {
        Dim3(dim3 { x: x, y: 1, z: 1 })
    }

    pub fn xy(x: u32, y: u32) -> Self {
        Dim3(dim3 { x: x, y: y, z: 1 })
    }

    pub fn xyz(x: u32, y: u32, z: u32) -> Self {
        Dim3(dim3 { x: x, y: y, z: z })
    }
}

#[derive(Debug, Clone, Copy, NewType)]
pub struct Block(Dim3);

#[derive(Debug, Clone, Copy, NewType)]
pub struct Grid(Dim3);

fn str2cstring(s: &str) -> Vec<c_char> {
    let cstr = String::from_str(s).unwrap() + "\0";
    cstr.into_bytes().into_iter().map(|c| c as c_char).collect()
}
