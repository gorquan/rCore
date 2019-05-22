use crate::lkm::ffi::*;
use alloc::sync::{Arc, Weak};
use alloc::boxed::Box;
use crate::rcore_fs::vfs::*;
use core::mem::{uninitialized};
use alloc::string::String;
use core::any::Any;
use alloc::collections::btree_map::BTreeMap;
use spin::RwLock;
use crate::lkm::api::cstr_to_str;
use alloc::vec::Vec;

// The basic idea behind the INode-struct is that the INode-struct in memory works as a cache.
// 1. Every INode struct in memory corresponds to an INode on disk.
// 2. One disk inode only has one mirror in memory. The mirror is created on first load into memory and destroyed (when necessary?)
// 3. Filesystem maintains an INode-struct table. Both self-reference counting(doing RC on your own) and reuse Arc (lkm_api_clone_arcinode) are OK.
// 4. When last reference to an INode cache item destroyed, you can do some cleanup.
//    Here comes the problem: does this indicate that the inode is destroyed?
//    1) INode cache never invalidates. This means the inode is destroyed. For ramfs and sfs here we need some cleanup.
//    2) INode cache may invalidate. This means the inode is probably not destroyed, but only released from kernel. You can check nlinks to decide whether to cleanup.
//       But why do you invalidate the item when it cannot save you any memory (since the inode cache item will not be freed)?
//       One possible situation: Your filesystem does not keep track of the entire INode table but creates the INode only when necessary. (Ahh now you need a hashtable.)
//       For example, you may like this on a network FS.
//    3) How should we handle this on a safe journal FS? (Imagine that after unlinking an opened file the system crashes.)
//       An approach is that when unlinking, move the inode to some danger zone.
//       When inode in danger zone finally unlinked from running kernel, move the inode to recycle zone.
//       Every reboot moves entire danger zone to recycle zone.
//  So for absolute safety it is not suggested to use this mechanism to handle cleanup on disk, but rather hold the lifecycle of inodes in your own hand.
#[repr(C)]
#[derive(Clone)]
pub struct INodeOperations{
    pub read_at: extern "C" fn (inode: usize, offset: usize, buf: usize, len: usize) -> isize,
    pub write_at: extern "C" fn(inode: usize, offset: usize, buf: usize, len: usize) -> isize,
    pub metadata: extern "C" fn (inode: usize, metadata: usize) -> isize,
    pub set_metadata: extern "C" fn (inode: usize, metadata: usize)->isize,
    pub poll: extern "C" fn (inode: usize, status: usize) -> isize,
    /// Sync all data and metadata
    pub sync_all: extern "C" fn(inode: usize) -> isize,
    /// Sync data (not include metadata)
    pub sync_data: extern "C" fn(inode: usize) -> isize,
    pub resize: extern "C" fn(inode: usize, len: usize) -> isize,
    pub create: extern "C" fn(inode: usize, name: usize, len:usize, type_: usize, mode: u32, result: usize) -> isize, // raw-inode.
    pub setrdev: extern "C" fn(inode: usize, dev:u64)->isize,
    pub unlink: extern "C" fn(inode: usize, name: usize, len: usize) -> isize,
    /// user of the vfs api should call borrow_mut by itself
    pub link: extern "C" fn(inode: usize, name: usize, len: usize, other: usize) -> isize,
    /// Move INode `self/old_name` to `target/new_name`.
    /// If `target` equals `self`, do rename.
    pub move_: extern "C" fn(inode: usize, old_name: usize, old_len: usize, target: usize, new_name: usize, new_len: usize) -> isize,
    /// lookup with only one layer
    pub find: extern "C" fn(inode: usize, name: usize, len: usize, result: usize) -> isize,
    /// like list()[id]
    /// only get one item in list, often faster than list
    pub get_entry: extern "C" fn(inode: usize, id: usize, buf: usize /*At least MAX_PATH!*/) -> isize,
    pub io_control: extern "C" fn(inode: usize, cmd: u32, data: usize) -> isize,
    // The real-dropping job is done.
    // This happens after the extern inode is released and last reference to it is destroyed.
    pub drop: extern "C" fn(inode:usize)
}

#[no_mangle]
pub extern "C" fn lkm_api_register_fs(name: usize, fsops: usize, inodeops: usize, fsdata: usize)->usize{
    let fsops=unsafe{&*(fsops  as *const FilesystemOperations)};
    let fsops=Arc::new(fsops.clone());
    let inodeops=unsafe{&*(inodeops as *const INodeOperations)};
    let inodeops=Arc::new(inodeops.clone());
    info!("Metadata: {}",inodeops.metadata as usize);
    let fs=ExternFileSystemType{
        inode_operations: inodeops,
        operations: fsops,
    };
    info!("Registering filesystem {}", unsafe{cstr_to_str(name as *const u8, 256)});

    FileSystemManager::get().write().registerFileSystem(&unsafe{cstr_to_str(name as *const u8, 256)}, fs);
    0
}
#[no_mangle]
pub extern "C" fn lkm_api_create_arc_inode(fs: usize, inode:usize)->usize{
    info!("lkm_api_create_arc_inode fs={} inode={}", fs, inode);
    let fs=unsafe{Arc::from_raw(fs as *const ExternFilesystem)};
    let extern_inode=(Arc::new(
        ExternINode{
            operations: Arc::clone(&fs.inode_operations),
            data: inode,
            filesystem: Arc::clone(&fs)
        }

    ));

    info!("metadata: {}", extern_inode.operations.metadata as usize);
    Arc::into_raw(fs);
    let ret=Arc::into_raw(extern_inode) as usize;
    info!("Created inode {}", ret);
    ret
}
#[no_mangle]
pub extern "C" fn lkm_api_release_arc_inode(arc_inode:usize){
    info!("Releasing inode {}", arc_inode);
    let extern_inode:Arc<ExternINode>=unsafe{Arc::from_raw(arc_inode as *const ExternINode)};
    // Dropping the inode.
}
#[no_mangle]
pub extern "C" fn lkm_api_clone_arc_inode(arc_inode:usize)->usize{
    info!("Cloning inode {}", arc_inode);
    let extern_inode:Arc<ExternINode>=unsafe{Arc::from_raw(arc_inode as *const ExternINode)};
    let ret=Arc::clone(&extern_inode);
    Arc::into_raw(extern_inode);
    Arc::into_raw(ret) as usize
}


#[repr(C)]
#[derive(Clone)]
pub struct FilesystemOperations{
    pub mount: extern "C" fn (flags: usize, dev_name: usize, data: usize, arc_efs: usize, result: usize)->isize,
    pub sync: extern "C" fn(fs: usize)->isize,
    pub root_inode: extern "C" fn(fs: usize, inode: usize),
    pub info:extern "C" fn(fs: usize, data: usize),
    pub drop: extern "C" fn(fs: usize)
}

pub struct ExternFilesystem{
    operations: Arc<FilesystemOperations>,
    inode_operations: Arc<INodeOperations>,
    data: usize
}
impl Drop for ExternFilesystem{
    fn drop(&mut self) {
        (self.operations.drop)(self.data);
    }
}

pub struct FileSystemManager{
    fstypes: BTreeMap<String, Box<FileSystemType>>

}
pub static mut FS_MANAGER: Option<RwLock<FileSystemManager>>=None;
impl FileSystemManager{
    pub fn new()->FileSystemManager{
        FileSystemManager{
            fstypes: BTreeMap::new()
        }
    }
    pub fn init(){
        unsafe{
            FS_MANAGER=Some(RwLock::new(FileSystemManager::new()));
        }
    }
    pub fn get()->&'static RwLock<FileSystemManager>{
        unsafe {FS_MANAGER.as_ref().unwrap()}
    }
    pub fn registerFileSystem<T: FileSystemType + 'static>(&mut self, name: &str, fstype: T){
        self.fstypes.insert(String::from(name), Box::new(fstype));
    }
    pub fn mountFilesystem(&self, source: &str, fstype: &str, flags: u64, data: usize)->Result<Arc<FileSystem>> {
        if self.fstypes.contains_key(fstype){
            let fst=self.fstypes.get(fstype).unwrap();
            fst.mount(source, flags, data)
        }else {Err(FsError::InvalidParam)}
    }
}
pub trait FileSystemType{
    fn mount(&self, source: &str, flags: u64, data: usize)->Result<Arc<FileSystem>>;
}
struct ExternFileSystemType{
    operations: Arc<FilesystemOperations>,
    inode_operations: Arc<INodeOperations>
}
impl FileSystemType for ExternFileSystemType{
    fn mount(&self, source: &str, flags: u64, data: usize)->Result<Arc<FileSystem>>{
        let mut result:usize=0;
        let mut efs=(Arc::new(ExternFilesystem{
            operations: Arc::clone(&self.operations),
            inode_operations: Arc::clone(&self.inode_operations),
            data: 0
        }));
        let pefs=Arc::into_raw(efs);
        info!("created efs at {}", pefs as usize);
        let ret=(self.operations.mount)(flags as usize, source.as_ptr() as usize, data, pefs as usize, (&mut result as *mut usize) as usize);
        patch_isize_to_empty(ret)?;
        unsafe{(&mut *(pefs as *mut ExternFilesystem)).data=result;};

        let mut efs=unsafe{Arc::from_raw(pefs)};
        info!("mount done");
        //refefs.data=result;
        Ok(efs)
    }
    // TODO: unmount?
    // Dropping the filesystem is a good idea: but how can you find the filesystem by the device, which is allowed in umount(8)?
}
impl FileSystem for ExternFilesystem{
    fn sync(&self) -> Result<()> {
        patch_isize_to_empty((self.operations.sync)(self.data))
    }

    fn root_inode(&self) -> Arc<INode> {
        let mut inode:usize=0;
        (self.operations.root_inode)(self.data, (&mut inode as *mut usize) as usize);
        let ei=unsafe{Arc::from_raw(inode as *const ExternINode)};
        info!("root_inode of {} is {}", self.data, ei.data);
        ei
    }

    fn info(&self) -> FsInfo {
        let mut data:FsInfoFFI=unsafe{uninitialized()};
        (self.operations.info)(self.data, (&mut data as *mut FsInfoFFI) as usize);
        FsInfo::fromPrimitive(&data)
    }
}
// representing some INode delegated to external environment, especially kernel module.
pub struct ExternINode{
    operations: Arc<INodeOperations>,
    data: usize,
    filesystem: Arc<ExternFilesystem>
}
impl Drop for ExternINode{
    fn drop(&mut self) {
        info!("inode {} is dropped. this means arc=0", self.data);
        (self.operations.drop)(self.data);
    }
}

fn copy_to_cstr(s: &str)->Vec<u8>{
    let mut v=Vec::from(String::from(s));
    v.push(0);
    v
}

impl INode for ExternINode{
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>{
        info!("Calling external read_at at {}", self.operations.read_at as usize);
        let ret=(self.operations.read_at)(self.data, offset, buf.as_mut_ptr() as usize, buf.len());
        patch_isize_to_usize(ret)
    }
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>{
        info!("Calling external write_at at {} with argument {} {} {} {}", self.operations.write_at as usize, self.data as usize, offset, buf.as_ptr() as usize, buf.len());
        let ret=(self.operations.write_at)(self.data, offset, buf.as_ptr() as usize, buf.len());
        patch_isize_to_usize(ret)
    }
    fn poll(&self) -> Result<PollStatus> {
        info!("Calling external poll at {}", self.operations.poll as usize);
        let mut data: PollStatusFFI=unsafe{uninitialized()};
        let ret=(self.operations.poll)(self.data, &mut data as *mut PollStatusFFI as usize);
        patch_isize_to_empty(ret)?;
        Ok(PollStatus::fromPrimitive(&data))
    }
    fn metadata(&self) -> Result<Metadata>{
        info!("Calling external metadata at {} with data {}", self.operations.metadata as usize, self.data);
        let mut data: MetadataFFI=unsafe{uninitialized()};
        let ret=(self.operations.metadata)(self.data, &mut data as *mut MetadataFFI as usize);
        patch_isize_to_empty(ret)?;
        Ok(Metadata::fromPrimitive(&data))
    }
    fn set_metadata(&self, metadata: &Metadata) -> Result<()>{
        info!("Calling external set_metadata at {}", self.operations.set_metadata as usize);
        let data=metadata.toPrimitive();
        let ret=(self.operations.set_metadata)(self.data, &data as *const MetadataFFI as usize);
        patch_isize_to_empty(ret)
    }
    /// Sync all data and metadata
    fn sync_all(&self) -> Result<()>{
        info!("Calling external sync_all at {}", self.operations.sync_all as usize);
        let ret=(self.operations.sync_all)(self.data);
        patch_isize_to_empty(ret)
    }
    /// Sync data (not include metadata)
    fn sync_data(&self) -> Result<()>{
        info!("Calling external sync_data at {}", self.operations.sync_data as usize);
        let ret=(self.operations.sync_data)(self.data);
        patch_isize_to_empty(ret)
    }
    fn resize(&self, len: usize) -> Result<()>{
        info!("Calling external resize at {}", self.operations.resize as usize);
        let ret=(self.operations.resize)(self.data, len);
        patch_isize_to_empty(ret)
    }
    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>>{
        info!("Calling external create at {}", self.operations.create as usize);
        let mut inode:usize=0;
        let vname=copy_to_cstr(name);
        let ret=(self.operations.create)(self.data, vname.as_ptr() as usize, name.len(), FileType::toPrimitive(&type_), mode, &mut inode as *mut usize as usize);
        patch_isize_to_empty(ret)?;
        Ok(unsafe{Arc::from_raw(inode as *const ExternINode)})

    }
    /// user of the vfs api should call borrow_mut by itself
    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()>{
        info!("Calling external link at {}", self.operations.link as usize);
        if !Arc::ptr_eq(&self.fs(), &other.fs()){
            return Err(FsError::NotSameFs);
        }
        let peer=other.as_any_ref().downcast_ref::<ExternINode>().unwrap();
        let vname=copy_to_cstr(name);
        patch_isize_to_empty((self.operations.link)(self.data, vname.as_ptr() as usize, name.len(), peer.data))
    }
    fn unlink(&self, name: &str) -> Result<()>{
        info!("Calling external unlink at {}", self.operations.unlink as usize);
        let vname=copy_to_cstr(name);
        patch_isize_to_empty((self.operations.unlink)(self.data, vname.as_ptr() as usize, name.len()))
    }
    /// Move INode `self/old_name` to `target/new_name`.
    /// If `target` equals `self`, do rename.
    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>{
        info!("Calling external move_ at {}", self.operations.move_ as usize);
        if !Arc::ptr_eq(&self.fs(), &target.fs()){
            return Err(FsError::NotSameFs);
        }
        let peer=target.as_any_ref().downcast_ref::<ExternINode>().unwrap();
        let vold_name=copy_to_cstr(old_name);
        let vnew_name=copy_to_cstr(new_name);
        patch_isize_to_empty((self.operations.move_)(self.data, vold_name.as_ptr() as usize, old_name.len(), peer.data, vnew_name.as_ptr() as usize, new_name.len()))
    }
    /// lookup with only one layer
    fn find(&self, name: &str) -> Result<Arc<INode>>{
        info!("Calling external find at {} with argument {} {}, {}", self.operations.find as usize, self.data, name.as_ptr() as usize, name);
        let mut inode:usize=0;
        let vname=copy_to_cstr(name);
        let ret=(self.operations.find)(self.data, vname.as_ptr() as usize, name.len(), &mut inode as *mut usize as usize);
        patch_isize_to_empty(ret)?;
        let result=unsafe{Arc::from_raw(inode as *const ExternINode )};
        info!("{} found with data {}", name, result.data);
        Ok(result)
    }
    /// like list()[id]
    /// only get one item in list, often faster than list
    fn get_entry(&self, id: usize) -> Result<String>{
        info!("Calling external get_entry with argument {} {}", self.data, id);
        let mut buf:[u8;256]=unsafe{uninitialized()};
        let ret=(self.operations.get_entry)(self.data, id, buf.as_mut_ptr() as usize);
        patch_isize_to_empty(ret)?;
        let nul_range_end = buf.iter()
            .position(|&c| c == b'\0')
            .unwrap_or(buf.len()); // default to length if no `\0` present
        Ok(String::from(unsafe{alloc::str::from_utf8_unchecked(&buf[0..nul_range_end])}))
    }
    fn io_control(&self, cmd: u32, data: usize) -> Result<()> {
        println!("calling external ioctl");
        patch_isize_to_empty((self.operations.io_control)(self.data, cmd, data))
    }
    //    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
    fn fs(&self) -> Arc<FileSystem>{self.filesystem.clone()}

    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_ref(&self) -> &Any{return self;}

    fn setrdev(&self, dev:u64)->Result<()>{
        info!("Calling external setrdev at {}", self.operations.setrdev as usize);
        patch_isize_to_empty((self.operations.setrdev)(self.data, dev))
    }
}