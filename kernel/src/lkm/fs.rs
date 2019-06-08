use crate::fs::vfs::*;
use crate::syscall::Syscall;
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::any::Any;
use core::mem::uninitialized;
use rcore_fs::vfs::{FileSystem, FsError, Result};
use spin::RwLock;

pub struct FileSystemManager {
    fstypes: BTreeMap<String, Box<FileSystemType>>,
}

pub static mut FS_MANAGER: Option<RwLock<FileSystemManager>> = None;
impl FileSystemManager {
    pub fn new() -> FileSystemManager {
        FileSystemManager {
            fstypes: BTreeMap::new(),
        }
    }
    pub fn init() {
        unsafe {
            FS_MANAGER = Some(RwLock::new(FileSystemManager::new()));
        }
        let mut fsm = Self::get().write();
        //fsm.registerFileSystem("sfs", crate::rcore_fs_sfs::SimpleFileSystemType{});
        //RamFSBehav::registerRamFS();
    }
    pub fn get() -> &'static RwLock<FileSystemManager> {
        unsafe { FS_MANAGER.as_ref().unwrap() }
    }
    pub fn registerFileSystem<T: FileSystemType + 'static>(&mut self, name: &str, fstype: T) {
        self.fstypes.insert(String::from(name), Box::new(fstype));
    }
    pub fn mountFilesystem(
        &self,
        syscall: &mut Syscall,
        source: &str,
        fstype: &str,
        flags: u64,
        data: usize,
    ) -> Result<Arc<FileSystem>> {
        if self.fstypes.contains_key(fstype) {
            let fst = self.fstypes.get(fstype).unwrap();
            fst.mount(syscall, source, flags, data)
        } else {
            Err(FsError::InvalidParam)
        }
    }
}
pub trait FileSystemType {
    fn mount(
        &self,
        syscall: &mut Syscall,
        source: &str,
        flags: u64,
        data: usize,
    ) -> Result<Arc<FileSystem>>;
}
