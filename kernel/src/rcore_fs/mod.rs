#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]
#![feature(const_str_len)]

extern crate alloc;

pub mod dev;
pub mod dirty;
pub mod file;
pub mod util;
pub mod vfs;
use alloc::sync::Arc;
use core::mem::uninitialized;
use spin::RwLock;
//lazy_static!{
//    pub static ref VIRTUAL_FS: Arc<RwLock<vfs::VirtualFS>>=vfs::VirtualFS::init();
//}

pub static mut VIRTUAL_FS: Option<Arc<RwLock<vfs::VirtualFS>>> = None;

pub fn init() {
    unsafe {
        VIRTUAL_FS = Some(vfs::VirtualFS::init());
        vfs::ANONYMOUS_FS = Some(Arc::new(RwLock::new(unsafe { uninitialized() })));
    }
}

pub fn get_virtual_fs() -> &'static Arc<RwLock<vfs::VirtualFS>> {
    unsafe { VIRTUAL_FS.as_ref().unwrap() }
}
