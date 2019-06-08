use alloc::{sync::Arc, vec::Vec};

use rcore_fs::dev::block_cache::BlockCache;
use rcore_fs::vfs::*;
use rcore_fs_sfs::SimpleFileSystem;

use crate::drivers::BlockDriver;

pub use self::file::*;
pub use self::file_like::*;
pub use self::pipe::Pipe;
pub use self::pseudo::*;
pub use self::stdio::{STDIN, STDIN_INODE, STDOUT, STDOUT_INODE};
pub use self::vga::*;
use core::mem::uninitialized;
use spin::RwLock;

mod device;
mod file;
mod file_like;
mod ioctl;
mod pipe;
mod pseudo;
mod stdio;
pub mod vfs;
pub mod vga;

// Hard link user programs
#[cfg(feature = "link_user")]
global_asm!(concat!(
    r#"
	.section .data.img
	.global _user_img_start
	.global _user_img_end
_user_img_start:
    .incbin ""#,
    env!("SFSIMG"),
    r#""
_user_img_end:
"#
));

pub const FOLLOW_MAX_DEPTH: usize = 1;

pub trait INodeExt {
    fn read_as_vec(&self) -> Result<Vec<u8>>;
}

impl INodeExt for INode {
    fn read_as_vec(&self) -> Result<Vec<u8>> {
        let size = self.metadata()?.size;
        let mut buf = Vec::with_capacity(size);
        unsafe {
            buf.set_len(size);
        }
        self.read_at(0, buf.as_mut_slice())?;
        Ok(buf)
    }
}

pub static mut VIRTUAL_FS: Option<Arc<RwLock<vfs::VirtualFS>>> = None;

pub fn init() {
    unsafe {
        VIRTUAL_FS = Some(vfs::VirtualFS::init());
        // XXX: ???
        vfs::ANONYMOUS_FS = Some(Arc::new(RwLock::new(unsafe { uninitialized() })));
    }
}

pub fn get_virtual_fs() -> &'static Arc<RwLock<vfs::VirtualFS>> {
    unsafe { VIRTUAL_FS.as_ref().unwrap() }
}
