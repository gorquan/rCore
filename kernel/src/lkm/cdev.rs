// cdev-alike interface for device managing.

use crate::rcore_fs::vfs::{Result, Metadata, INode, FileSystem, FileType, PollStatus, INodeContainer};
use crate::fs::{FileHandle, SeekFrom, FileLike, OpenOptions};
use alloc::sync::Arc;
use alloc::boxed::Box;
use alloc::string::String;
use core::any::Any;
use alloc::collections::btree_map::BTreeMap;
use crate::lkm::structs::ModuleRef;
use spin::RwLock;

pub struct INodeOperations{
    pub read_at: extern "C" fn (inode: usize, offset: usize, buf: &mut [u8]) -> Result<usize>,
    pub write_at: extern "C" fn(inode: usize, offset: usize, buf: &[u8]) -> Result<usize>,
    pub metadata: extern "C" fn (inode: usize) -> Result<Metadata>,
    pub set_metadata: extern "C" fn (inode: usize, metadata: &Metadata)->Result<()>,
    pub poll: extern "C" fn (inode: usize) -> Result<PollStatus>,
    /// Sync all data and metadata
    pub sync_all: extern "C" fn(inode: usize) -> Result<()>,
    /// Sync data (not include metadata)
    pub sync_data: extern "C" fn(inode: usize) -> Result<()>,
    pub resize: extern "C" fn(inode: usize, len: usize) -> Result<()>,
    pub create: extern "C" fn(inode: usize, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>>,
    pub setrdev: extern "C" fn(inode: usize, dev:u64)->Result<()>,
    pub unlink: extern "C" fn(inode: usize, name: &str) -> Result<()>,
    /// user of the vfs api should call borrow_mut by itself
    pub link: extern "C" fn(inode: usize, name: &str, other: &Arc<INode>) -> Result<()>,
    /// Move INode `self/old_name` to `target/new_name`.
    /// If `target` equals `self`, do rename.
    pub move_: extern "C" fn(inode: usize, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>,
    /// lookup with only one layer
    pub find: extern "C" fn(inode: usize, name: &str) -> Result<Arc<INode>>,
    /// like list()[id]
    /// only get one item in list, often faster than list
    pub get_entry: extern "C" fn(inode: usize, id: usize) -> Result<String>,
    //    io_ctrl: fn(&mut self, op: u32, data: &[u8]) -> Result<()>,
    pub fs: extern "C" fn(inode: usize) -> Arc<FileSystem>,
    pub io_control: extern "C" fn(inode: usize, cmd: u32, data: usize) -> Result<()>

}

// representing some INode delegated to external environment, especially kernel module.
pub struct ExternINode{
    operations: Arc<INodeOperations>,
    data: usize
}

impl INode for ExternINode{
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>{(self.operations.read_at)(self.data, offset, buf)}
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>{(self.operations.write_at)(self.data, offset, buf)}
    fn poll(&self) -> Result<PollStatus> {
        (self.operations.poll)(self.data)
    }
    fn metadata(&self) -> Result<Metadata>{(self.operations.metadata)(self.data)}
    fn set_metadata(&self, metadata: &Metadata) -> Result<()>{
        (self.operations.set_metadata)(self.data, metadata)
    }
    /// Sync all data and metadata
    fn sync_all(&self) -> Result<()>{(self.operations.sync_all)(self.data)}
    /// Sync data (not include metadata)
    fn sync_data(&self) -> Result<()>{(self.operations.sync_data)(self.data)}
    fn resize(&self, len: usize) -> Result<()>{(self.operations.resize)(self.data, len)}
    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>>{(self.operations.create)(self.data, name, type_, mode)}
    /// user of the vfs api should call borrow_mut by itself
    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()>{(self.operations.link)(self.data, name, other)}
    fn unlink(&self, name: &str) -> Result<()>{(self.operations.unlink)(self.data, name)}
    /// Move INode `self/old_name` to `target/new_name`.
    /// If `target` equals `self`, do rename.
    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>{(self.operations.move_)(self.data, old_name, target, new_name)}
    /// lookup with only one layer
    fn find(&self, name: &str) -> Result<Arc<INode>>{(self.operations.find)(self.data, name)}
    /// like list()[id]
    /// only get one item in list, often faster than list
    fn get_entry(&self, id: usize) -> Result<String>{(self.operations.get_entry)(self.data, id)}
    fn io_control(&self, cmd: u32, data: usize) -> Result<()> {
        (self.operations.io_control)(self.data, cmd, data)
    }
    //    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
    fn fs(&self) -> Arc<FileSystem>{(self.operations.fs)(self.data)}

    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_ref(&self) -> &Any{return self;}

    fn setrdev(&self, dev:u64)->Result<()>{(self.operations.setrdev)(self.data, dev)}
}
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
        println!("Registering device for {}", dev);
        self.dev_map.insert(dev, Arc::new(RwLock::new(device)));
    }
    pub fn openDevice(&self, inode_container: Arc<INodeContainer>, options: OpenOptions)->Result<FileLike>{
        let dev=inode_container.inode.metadata()?.rdev;
        println!("Finding device {} {} {}", dev, dev_major(dev), dev_minor(dev));
        let cdev=self.dev_map.get(&dev_major(dev)).ok_or(FsError::NoDevice)?;
        Ok(FileLike::File(FileHandle::new_with_cdev(inode_container, options, cdev)))
    }
    pub fn get()->&'static RwLock<CDevManager>{
        unsafe {CDEV_MANAGER.as_ref().unwrap()}
    }
}

