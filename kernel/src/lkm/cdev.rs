// cdev-alike interface for device managing.

use crate::rcore_fs::vfs::{Result, Metadata, INode, FileSystem, FileType, PollStatus, INodeContainer, FsInfo};
use crate::fs::{FileHandle, SeekFrom, FileLike, OpenOptions};
use alloc::sync::Arc;
use alloc::boxed::Box;
use alloc::string::String;
use core::any::Any;
use alloc::collections::btree_map::BTreeMap;
use crate::lkm::structs::ModuleRef;
use spin::RwLock;
use core::mem::transmute;

/*
pub trait FileOperations {
    pub open: Option<fn()->usize>,
    pub read: Option<fn(file: usize, buf: &mut [u8]) -> Result<usize>>,
    pub read_at: Option<fn(file: usize, offset: usize, buf: &mut [u8]) -> Result<usize>>,
    pub write: Option<fn(file: usize, buf: &[u8]) -> Result<usize>>,
    pub write_at: Option<fn(file: usize, offset: usize, buf: &[u8]) -> Result<usize>>,
    pub seek: Option<fn(file: usize, pos: SeekFrom) -> Result<u64>>,
    pub set_len: Option<fn(file: usize, len: u64) -> Result<()>>,
    pub sync_all: Option<fn(file: usize) -> Result<()>>,
    pub sync_data: Option<fn(file: usize) -> Result<()>>,
    pub metadata: Option<fn(file: usize) -> Result<Metadata>>,
    pub read_entry: Option<fn(file: usize) -> Result<String>>,
    pub poll: Option<fn (file: usize) -> Result<PollStatus>>,
    pub io_control: Option<fn(file: usize, cmd: u32, data: usize) -> Result<()>>,
    pub close: Option<fn(file: usize)>
}

*/
pub trait FileOperations: Send+Sync{
    fn open(&self)->usize;
    fn read(&self, fh: &mut FileHandle, buf: &mut [u8]) -> Result<usize>;
    fn read_at(&self, fh: &mut FileHandle, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, fh: &mut FileHandle, buf: &[u8]) -> Result<usize>;
    fn write_at(&self, fh: &mut FileHandle, offset: usize, buf: &[u8]) -> Result<usize> ;
    fn seek(&self, fh: &mut FileHandle, pos: SeekFrom) -> Result<u64> ;
    fn set_len(&self, fh: &mut FileHandle, len: u64) -> Result<()> ;
    fn sync_all(&self, fh: &mut FileHandle) -> Result<()> ;
    fn sync_data(&self, fh: &mut FileHandle) -> Result<()> ;
    fn metadata(&self, fh: &FileHandle) -> Result<Metadata> ;
    fn read_entry(&self, fh: &mut FileHandle) -> Result<String> ;
    fn poll(&self, fh: &FileHandle) -> Result<PollStatus> ;
    fn io_control(&self, fh: &FileHandle, cmd: u32, arg: usize) -> Result<()> ;
    fn close(&self, data: usize);
}

pub fn dev_major(dev: u64)->u32{
    ((dev>>8)&0x7f) as u32
}
pub fn dev_minor(dev: u64)->u32{
    (dev&0xff) as u32
}
pub struct CharDev{
    pub parent_module: Option<Arc<ModuleRef>>,
    pub file_op: Arc<FileOperations>
}


pub struct CDevManager{
    dev_map: BTreeMap<u32, Arc<RwLock<CharDev>> >
}
pub type LockedCharDev=RwLock<CharDev>;
pub static mut CDEV_MANAGER: Option<RwLock<CDevManager>>=None;
use crate::rcore_fs::vfs::{FsError};
use core::cell::RefCell;
use crate::sync::SpinNoIrqLock as Mutex;
use crate::lkm::ffi::*;
use core::mem;
use core::mem::uninitialized;

impl CDevManager{
    pub fn new()->CDevManager{
        CDevManager{
            dev_map: BTreeMap::new()
        }
    }
    pub fn init(){
        unsafe{
            CDEV_MANAGER=Some(RwLock::new(CDevManager::new()));
        }
        let mut cdevm=CDevManager::get().write();
        //cdevm.registerDevice(20, super::hello_device::get_cdev());
    }
    pub fn registerDevice(&mut self, dev: u32, device: CharDev){
        info!("Registering device for {}", dev);
        self.dev_map.insert(dev, Arc::new(RwLock::new(device)));
    }
    pub fn openDevice(&self, inode_container: Arc<INodeContainer>, options: OpenOptions)->Result<FileLike>{
        let dev=inode_container.inode.metadata()?.rdev;
        info!("Finding device {} {} {}", dev, dev_major(dev), dev_minor(dev));
        let cdev=self.dev_map.get(&dev_major(dev)).ok_or(FsError::NoDevice)?;
        Ok(FileLike::File(FileHandle::new_with_cdev(inode_container, options, cdev)))
    }
    pub fn get()->&'static RwLock<CDevManager>{
        unsafe {CDEV_MANAGER.as_ref().unwrap()}
    }
}

