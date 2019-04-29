// cdev-alike interface for device managing.

use crate::rcore_fs::vfs::{Result, Metadata, INode, FileSystem, FileType};
use crate::fs::{FileHandle, SeekFrom};
use alloc::sync::Arc;
use alloc::string::String;
use core::any::Any;
use alloc::collections::btree_map::BTreeMap;
use crate::lkm::structs::ModuleRef;

pub struct INodeOperations{
    pub read_at: extern "C" fn (inode: usize, offset: usize, buf: &mut [u8]) -> Result<usize>,
    pub write_at: extern "C" fn(inode: usize, offset: usize, buf: &[u8]) -> Result<usize>,
    pub metadata: extern "C" fn (inode: usize) -> Result<Metadata>,
    pub chmod: extern "C" fn(inode: usize, mode: u16) -> Result<()>,
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

}

// representing some INode delegated to external environment, especially kernel module.
pub struct ExternINode{
    operations: Arc<INodeOperations>,
    data: usize
}

impl INode for ExternINode{
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>{(self.operations.read_at)(self.data, offset, buf)}
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>{(self.operations.write_at)(self.data, offset, buf)}
    fn metadata(&self) -> Result<Metadata>{(self.operations.metadata)(self.data)}
    fn chmod(&self, mode: u16) -> Result<()>{(self.operations.chmod)(self.data, mode)}
    /// Sync all data and metadata
    fn sync_all(&self) -> Result<()>{(self.operations.sync_all)(self.data)}
    /// Sync data (not include metadata)
    fn sync_data(&self) -> Result<()>{(self.operations.sync_data)(self.data)}
    fn resize(&self, len: usize) -> Result<()>{(self.operations.resize)(self.data, len)}
    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>>{(self.operations.create)(self.data, name, type_, mode)}
    fn setrdev(&self, dev:u64)->Result<()>{(self.operations.setrdev)(self.data, dev)}
    fn unlink(&self, name: &str) -> Result<()>{(self.operations.unlink)(self.data, name)}
    /// user of the vfs api should call borrow_mut by itself
    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()>{(self.operations.link)(self.data, name, other)}
    /// Move INode `self/old_name` to `target/new_name`.
    /// If `target` equals `self`, do rename.
    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>{(self.operations.move_)(self.data, old_name, target, new_name)}
    /// lookup with only one layer
    fn find(&self, name: &str) -> Result<Arc<INode>>{(self.operations.find)(self.data, name)}
    /// like list()[id]
    /// only get one item in list, often faster than list
    fn get_entry(&self, id: usize) -> Result<String>{(self.operations.get_entry)(self.data, id)}
    //    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
    fn fs(&self) -> Arc<FileSystem>{(self.operations.fs)(self.data)}
    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_ref(&self) -> &Any{return self;}
}

pub struct FileOperations {
    pub open: Option<extern "C" fn(file: &mut FileHandle)>,
    pub read: Option<extern "C" fn(file: &mut FileHandle, buf: &mut [u8]) -> Result<usize>>,
    pub read_at: Option<extern "C" fn(file: &mut FileHandle, offset: usize, buf: &mut [u8]) -> Result<usize>>,
    pub write: Option<extern "C" fn(file: &mut FileHandle, buf: &[u8]) -> Result<usize>>,
    pub write_at: Option<extern "C" fn(file: &mut FileHandle, offset: usize, buf: &[u8]) -> Result<usize>>,
    pub seek: Option<extern "C" fn(file: &mut FileHandle, pos: SeekFrom) -> Result<u64>>,
    pub set_len: Option<extern "C" fn(file: &mut FileHandle, len: u64) -> Result<()>>,
    pub sync_all: Option<extern "C" fn(file: &mut FileHandle) -> Result<()>>,
    pub sync_data: Option<extern "C" fn(file: &mut FileHandle) -> Result<()>>,
    pub metadata: Option<extern "C" fn(file: &FileHandle) -> Result<Metadata>>,
    pub read_entry: Option<extern "C" fn(file: &mut FileHandle) -> Result<String>>
}

pub struct CharDev{
    parent_module: Option<Arc<ModuleRef>>,
    file_op: Arc<FileOperations>
}
pub struct CDevManager{
    dev_map: BTreeMap<u32, Box<CharDev>>
}