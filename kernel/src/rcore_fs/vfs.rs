use alloc::{string::String, sync::Arc, vec::Vec};
use core::any::Any;
use core::fmt;
use core::result;
use core::str;

/// Abstract operations on a inode.
pub trait INode: Any + Sync + Send {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn metadata(&self) -> Result<Metadata>;
    fn chmod(&self, mode: u16) -> Result<()>;
    /// Sync all data and metadata
    fn sync_all(&self) -> Result<()>;
    /// Sync data (not include metadata)
    fn sync_data(&self) -> Result<()>;
    fn resize(&self, len: usize) -> Result<()>;
    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>>;
    fn setrdev(&self, dev:u64)->Result<()>{
        unimplemented!()
    }
    fn unlink(&self, name: &str) -> Result<()>;
    /// user of the vfs api should call borrow_mut by itself
    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()>;
    /// Move INode `self/old_name` to `target/new_name`.
    /// If `target` equals `self`, do rename.
    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>;
    /// lookup with only one layer
    fn find(&self, name: &str) -> Result<Arc<INode>>;
    /// like list()[id]
    /// only get one item in list, often faster than list
    fn get_entry(&self, id: usize) -> Result<String>;
    //    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
    fn fs(&self) -> Arc<FileSystem>;
    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_ref(&self) -> &Any;
}

impl INode {
    pub fn downcast_ref<T: INode>(&self) -> Option<&T> {
        self.as_any_ref().downcast_ref::<T>()
    }
    pub fn list(&self) -> Result<Vec<String>> {
        let info = self.metadata()?;
        if info.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        (0..info.size).map(|i| self.get_entry(i)).collect()
    }
/*
    /// Lookup path from current INode, and do not follow symlinks
    pub fn lookup(&self, path: &str) -> Result<Arc<INode>> {
        self.lookup_follow(path, 0)
    }

    /// Lookup path from current INode, and follow symlinks at most `follow_times` times
    pub fn lookup_follow(&self, path: &str, mut follow_times: usize) -> Result<Arc<INode>> {
        if self.metadata()?.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }

        let mut result = self.find(".")?;
        let mut rest_path = String::from(path);
        while rest_path != "" {
            if result.metadata()?.type_ != FileType::Dir {
                return Err(FsError::NotDir);
            }
            // handle absolute path
            if let Some('/') = rest_path.chars().next() {
                result = self.fs().root_inode();
                rest_path = String::from(&rest_path[1..]);
                continue;
            }
            let name;
            match rest_path.find('/') {
                None => {
                    name = rest_path;
                    rest_path = String::new();
                }
                Some(pos) => {
                    name = String::from(&rest_path[0..pos]);
                    rest_path = String::from(&rest_path[pos + 1..]);
                }
            };
            match result.find(&name) {
                Err(error) => return Err(error),
                Ok(inode) => {
                    // Handle symlink
                    if inode.metadata()?.type_ == FileType::SymLink && follow_times > 0 {
                        follow_times -= 1;
                        let mut content = [0u8; 256];
                        let len = inode.read_at(0, &mut content)?;
                        if let Ok(path) = str::from_utf8(&content[..len]) {
                            // result remains unchanged
                            rest_path = {
                                let mut new_path = String::from(path);
                                if let Some('/') = new_path.chars().last() {
                                    new_path += &rest_path;
                                } else {
                                    new_path += "/";
                                    new_path += &rest_path;
                                }
                                new_path
                            };
                        } else {
                            return Err(FsError::NotDir);
                        }
                    } else {
                        result = inode
                    }
                }
            };
        }
        Ok(result)
    }
    */
}

/// Metadata of INode
///
/// Ref: [http://pubs.opengroup.org/onlinepubs/009604499/basedefs/sys/stat.h.html]
#[derive(Debug, Eq, PartialEq)]
pub struct Metadata {
    /// Device ID
    pub dev: usize,
    /// Inode number
    pub inode: usize,
    /// Size in bytes
    ///
    /// SFS Note: for normal file size is the actuate file size
    /// for directory this is count of dirent.
    pub size: usize,
    /// A file system-specific preferred I/O block size for this object.
    /// In some file system types, this may vary from file to file.
    pub blk_size: usize,
    /// Size in blocks
    pub blocks: usize,
    /// Time of last access
    pub atime: Timespec,
    /// Time of last modification
    pub mtime: Timespec,
    /// Time of last change
    pub ctime: Timespec,
    /// Type of file
    pub type_: FileType,
    /// Permission
    pub mode: u16,
    /// Number of hard links
    ///
    /// SFS Note: different from linux, "." and ".." count in nlinks
    /// this is same as original ucore.
    pub nlinks: usize,
    /// User ID
    pub uid: usize,
    /// Group ID
    pub gid: usize,
    pub rdev: u64
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Timespec {
    pub sec: i64,
    pub nsec: i32,
}

#[derive(Debug, Eq, PartialEq)]
pub enum FileType {
    File,
    Dir,
    SymLink,
    CharDevice,
    BlockDevice,
    NamedPipe,
    Socket,
}

/// Metadata of FileSystem
///
/// Ref: [http://pubs.opengroup.org/onlinepubs/9699919799/]
#[derive(Debug)]
pub struct FsInfo {
    /// File system block size
    pub bsize: usize,
    /// Fundamental file system block size
    pub frsize: usize,
    /// Total number of blocks on file system in units of `frsize`
    pub blocks: usize,
    /// Total number of free blocks
    pub bfree: usize,
    /// Number of free blocks available to non-privileged process
    pub bavail: usize,
    /// Total number of file serial numbers
    pub files: usize,
    /// Total number of free file serial numbers
    pub ffree: usize,
    /// Maximum filename length
    pub namemax: usize,
}

// Note: IOError/NoMemory always lead to a panic since it's hard to recover from it.
//       We also panic when we can not parse the fs on disk normally
#[derive(Debug)]
pub enum FsError {
    NotSupported,  //E_UNIMP, or E_INVAL
    NotFile,       //E_ISDIR
    IsDir,         //E_ISDIR, used only in link
    NotDir,        //E_NOTDIR
    EntryNotFound, //E_NOENT
    EntryExist,    //E_EXIST
    NotSameFs,     //E_XDEV
    InvalidParam,  //E_INVAL
    NoDeviceSpace, //E_NOSPC, but is defined and not used in the original ucore, which uses E_NO_MEM
    DirRemoved,    //E_NOENT, when the current dir was remove by a previous unlink
    DirNotEmpty,   //E_NOTEMPTY
    WrongFs,       //E_INVAL, when we find the content on disk is wrong when opening the device
    DeviceError,
    SymLoop        //E_LOOP, too many symlink follows.
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(any(test, feature = "std"))]
impl std::error::Error for FsError {}

pub type Result<T> = result::Result<T, FsError>;

/// Abstract filesystem
pub trait FileSystem: Sync+Send {
    fn sync(&self) -> Result<()>;
    fn root_inode(&self) -> Arc<INode>;
    fn info(&self) -> FsInfo;
    // TODO: good ways to force attribute? For example, a filesystem must contain an owner attribute pointing to a module.
}
use alloc::collections::btree_map::*;
use core::sync::*;
use once::*;
use spin::RwLock;
use alloc::boxed::Box;
use alloc::borrow::ToOwned;



use alloc::slice::*;
use core::mem::*;
use crate::rcore_fs_sfs::SimpleFileSystem;



pub struct VirtualFS{
    pub filesystem: Arc<FileSystem>,
    pub mountpoints: BTreeMap<usize, Arc<RwLock<VirtualFS>>>,
    pub self_mountpoint: Arc<INodeContainer>
}


pub struct INodeContainer{
    pub inode: Arc<INode>, //TODO: deref to this
    pub vfs: Arc<RwLock<VirtualFS>>
}


impl VirtualFS{
    pub fn init()->Arc<RwLock<VirtualFS>>{
        //panic!("FS");
        assert_has_not_been_called!("VirtualFS::init must be called only once.");
        //
        let mut vfs=VirtualFS{
            filesystem: VirtualFS::init_mount_sfs(),
            mountpoints: BTreeMap::new(),
            self_mountpoint: Arc::new(unsafe {uninitialized()}) // This should NOT EVER BE ACCESSED!
        };
        //vfs.init_mount_sfs();
        Arc::new(RwLock::new(vfs))
    }

    fn init_mount_sfs()->Arc<FileSystem>{
        let sfs= {
            #[cfg(not(feature = "link_user"))]
                let device = {
                #[cfg(any(target_arch = "riscv32", target_arch = "riscv64", target_arch = "x86_64"))]
                    {
                        crate::drivers::BLK_DRIVERS.read().iter()
                            .next().expect("Block device not found")
                            .clone()
                    }
                #[cfg(target_arch = "aarch64")]
                    {
                        unimplemented!()
                    }
            };
            #[cfg(feature = "link_user")]
                let device = {
                extern {
                    fn _user_img_start();
                    fn _user_img_end();
                }
                info!("SFS linked to kernel, from {:08x} to {:08x}", _user_img_start as usize, _user_img_end as usize);
                Arc::new(unsafe { device::MemBuf::new(_user_img_start, _user_img_end) })
            };

            let sfs = SimpleFileSystem::open(device).expect("failed to open SFS");
            sfs
        };
        sfs
    }
    pub fn getRootINode(vfs: &Arc<RwLock<VirtualFS>>)->Arc<INodeContainer>{
        Arc::new(INodeContainer{
            inode: vfs.read().filesystem.root_inode(),
            vfs: Arc::clone(vfs)
        })
    }
    pub fn overlayMountpoint(&self, ic: INodeContainer)->Arc<INodeContainer>{
        self.getOverlaidMountPoint(Arc::new(ic))
    }
    pub fn getOverlaidMountPoint(&self, ic: Arc<INodeContainer>)->Arc<INodeContainer>{
        let inode_id=ic.inode.metadata().unwrap().inode;
        if let Some(sub_vfs)=self.mountpoints.get(&inode_id){
            let mut sub_inode=VirtualFS::getRootINode(sub_vfs);
            //sub_inode.original_mountpoint=Some(Arc::new(ic));
            sub_inode
        }else{
            ic
        }
    }
}

pub struct PathConfig{
    pub root: Arc<INodeContainer>, // ensured to be a dir.
    pub cwd: Arc<INodeContainer> //ensured to be a dir.
}

// The enum used to represent result of a successful path resolve.
pub enum PathResolveResult{
    IsDir{ // You can always get the parent directory by inode, so no necessity to take with parent.
        dir: Arc<INodeContainer>
    },
    IsFile{ // If it is a file, its parent must have been touched.
            // This is also returned for further symbol resolving, since resolving symbol needs parent directory.
        file: Arc<INodeContainer>,
        parent: Arc<INodeContainer>
    },
    NotExist{ // If it is not found, its parent must have been touched. This is useful when dealing with syscalls like creat or rename.
        parent: Arc<INodeContainer>,
        name: String
    }

}
// Path resolution must be done with a root.
// A better name is "Filesystem Selector", like the "segment selector".
impl PathConfig{
    pub fn init_root()->PathConfig{
        let root=VirtualFS::getRootINode(super::get_virtual_fs());
        let cwd=Arc::clone(&root);
        PathConfig{root, cwd}
    }


    pub fn path_resolve(&self, cwd: &Arc<INodeContainer>, path: &str, resolve_last_symbol: bool)->Result<PathResolveResult>{
        let mut follow_counter=40;
        let depth_counter=10;
        let r=self.resolvePath(cwd, path, &mut follow_counter, depth_counter)?;
        if resolve_last_symbol{
            if let PathResolveResult::IsFile {file, parent}=r{
                self.resolveSymbolRecursively(&parent,  &file, &mut follow_counter, depth_counter)
            }else{
                Ok(r)
            }
        }else{
            Ok(r)
        }
    }


    pub fn resolveParent(&self, cwd: &Arc<INodeContainer>)->Arc<INodeContainer>{
        cwd.find(self.hasReachedRoot(&cwd), "..").unwrap() //There is no reason that this can fail, as long as cwd is really a directory.
    }

    // This call is used by getcwd() to detect possible leaks.
    // All files are organized in a big tree, so it will eventually achieve the root.
    pub unsafe fn forceResolveParent(&self, cwd: &Arc<INodeContainer>)->Arc<INodeContainer>{
        cwd.find(false, "..").unwrap()
    }

    pub fn resolvePath(&self, cwd: &Arc<INodeContainer>, path: &str, follow_counter: &mut usize, depth_counter: usize)->Result<PathResolveResult>{
        //println!("Path resolution {}", path);
        let mut cwd=Arc::clone({
            if path.starts_with("/"){
                &self.root
            }else{
                if cwd.inode.metadata().unwrap().type_!=FileType::Dir{
                    return Err(FsError::NotDir);
                }
                cwd //must be a dir, or an error will be thrown.

            }
        });
        let parts: Vec<&str>=path.split("/").collect();
        let (last_part, mid_part)=parts.split_last().unwrap();

        for part in mid_part.iter(){
            if *part==""{
                continue;
            }
            //println!("Resolve part: {}", part);
            let next=cwd.find(self.hasReachedRoot(&cwd), part)?;
            //println!("solve link");
            // Try solve symbolic link.
            let symlink_solve_result=self.resolveSymbolRecursively(&cwd, &next, follow_counter, depth_counter)?;
            match symlink_solve_result{
                PathResolveResult::IsDir {dir}=>{cwd=dir;}
                PathResolveResult::IsFile {..}=>{return Err(FsError::NotDir);}
                PathResolveResult::NotExist {..}=>{return Err(FsError::EntryNotFound);}
            }
        }
        //println!("Last part {}", last_part);
        // Resolving last part.
        let next=cwd.find(self.hasReachedRoot(&cwd), last_part);
        //println!("match next");
        match next{
            Ok(next)=>{
                //println!("Ok!");
                //No extra check needed, since extra work can be done through check.
                if next.inode.metadata().unwrap().type_==FileType::Dir{
                    Ok(PathResolveResult::IsDir{
                        dir: next
                    })
                }else{
                    Ok(PathResolveResult::IsFile {
                        parent: cwd,
                        file: next
                    })
                }
            }
            Err(FsError::EntryNotFound)=>Ok(PathResolveResult::NotExist {name: String::from(*last_part), parent: cwd}),
            Err(x)=>Err(x)
        }

    }

    // Resolves symbol by one layer.
    // TODO: Linux proc fs has some anti-POSIX magics here, like /proc/[pid]/root.
    // In those cases, those magics points to strange places, without following symlink rules.
    // This hack can be achieved here.
    pub fn resolveSymbol(&self, cwd: &Arc<INodeContainer>, symbol: &Arc<INodeContainer>, follow_counter: &mut usize, depth_counter: usize)->Result<PathResolveResult>{
        if depth_counter==0{
            return Err(FsError::SymLoop);
        }
        if *follow_counter>0{
            *follow_counter-=1;
            let mut content = [0u8; 256];
            let len = symbol.inode.read_at(0, &mut content)?;
            if let Ok(path) = str::from_utf8(&content[..len]) {
                self.resolvePath(cwd, path, follow_counter, depth_counter-1)

            } else {
                return Err(FsError::NotDir);
            }

        }else{
            Err(FsError::SymLoop)
        }
    }
    // Resolves symbol recursively. Note that a not-found will cause the resolved symbol pointing to the final file.
    pub fn resolveSymbolRecursively(&self, cwd: &Arc<INodeContainer>, symbol: &Arc<INodeContainer>, follow_counter: &mut usize, depth_counter: usize)->Result<PathResolveResult>{
        let mut current_symbol_dir=Arc::clone(cwd);
        let mut current_symbol=Arc::clone(symbol);
        while current_symbol.inode.metadata().unwrap().type_==FileType::SymLink{
            let resolve_result=self.resolveSymbol(&current_symbol_dir, &current_symbol, follow_counter, depth_counter)?;
            match resolve_result{
                PathResolveResult::NotExist{..}=>{return Ok(resolve_result);}
                PathResolveResult::IsDir {..}=>{return Ok(resolve_result);}
                PathResolveResult::IsFile {file, parent}=>{current_symbol=file; current_symbol_dir=parent;}
            }
        }
        if current_symbol.inode.metadata().unwrap().type_==FileType::Dir{
            Ok(PathResolveResult::IsDir {dir: current_symbol})

        }else{
            Ok(PathResolveResult::IsFile {file: current_symbol, parent: current_symbol_dir})
        }
    }
    pub fn hasReachedRoot(&self, current: &Arc<INodeContainer>)->bool{
        Arc::ptr_eq(&current.vfs, &self.root.vfs)
            && self.root.inode.metadata().unwrap().inode==current.inode.metadata().unwrap().inode
    }

}
impl Clone for INodeContainer{
    fn clone(&self) -> Self {
        INodeContainer{
            inode: Arc::clone(&self.inode),
            vfs: Arc::clone(&self.vfs)
        }
    }
}

impl Clone for PathConfig{
    fn clone(&self) -> Self {
        PathConfig{
            root: Arc::clone(&self.root),
            cwd: Arc::clone(&self.cwd)
        }
    }
}

pub static mut ANONYMOUS_FS: Option<Arc<RwLock<VirtualFS>>>=None;

pub fn get_anonymous_fs()->&'static Arc<RwLock<VirtualFS>>{
    unsafe{
        ANONYMOUS_FS.as_ref().unwrap()
    }
}



impl INodeContainer{
    pub fn is_very_root(self: &Arc<INodeContainer>)->bool{
        PathConfig::init_root().hasReachedRoot(self)
    }
    pub fn is_root_inode(&self)->bool{
        self.inode.fs().root_inode().metadata().unwrap().inode==self.inode.metadata().unwrap().inode
    }

    //Creates an anonymous inode. Should not be used as a location at any time, or be totally released at any time.
    pub unsafe fn anonymous_inode(inode: Arc<INode>)->Arc<INodeContainer>{
        Arc::new(INodeContainer{
            inode,
            vfs: Arc::clone(get_anonymous_fs())
        })

    }

    //Does a one-level finding.
    pub fn find(self: &Arc<INodeContainer>, root: bool, next: &str)->Result<Arc<INodeContainer>>{
        //println!("finding name {}", next);
        //println!("in {:?}", self.inode.list().unwrap());
        match next{
            ""=>Ok(Arc::clone(&self)),
            "."=>Ok(Arc::clone(&self)),
            ".."=>{ // Going Up
                // We need to check these things:
                // 1. Is going forward allowed, considering the current root?
                // 2. Is going forward trespassing the filesystem border, thus requires falling back to parent of original_mountpoint?
                // TODO: check going up.
                if root{
                    Ok(Arc::clone(&self))
                }
                else if self.is_root_inode(){ //Here is mountpoint.
                    self.vfs.read().self_mountpoint.find( root, "..")
                }else{
                    // Not trespassing filesystem border. Parent and myself in the same filesystem.
                    Ok(Arc::new(INodeContainer{
                        inode: self.inode.find(next)?, //Going up is handled by the filesystem. A better API?
                        vfs: Arc::clone(&self.vfs)
                    }))
                }
            }
            _=>{
                // Going down may trespass the filesystem border.
                // An INode replacement is required here.
                let next_ic=INodeContainer{
                    inode: self.inode.find(next)?,
                    vfs: Arc::clone(&self.vfs)
                };
                //println!("find Ok!");
                Ok(self.vfs.read().overlayMountpoint(next_ic))
            }

        }
    }

    pub fn findNameByChild(self: &Arc<INodeContainer>, child: &Arc<INodeContainer>)->Result<String>{
        for index in 0..{
            match self.inode.get_entry(index){
                Ok(name)=>{

                    match name.as_ref(){
                        "."=>{}
                        ".."=>{}
                        _=>{
                            let queryback=self.find(false, &name)?;
                            let queryback=self.vfs.read().getOverlaidMountPoint(queryback);
                            //TODO: mountpoint check!
                            //println!("checking name {}", name);
                            if Arc::ptr_eq(&queryback.vfs, &self.vfs) && queryback.inode.metadata().unwrap().inode == child.inode.metadata().unwrap().inode{
                                return Ok(name);
                            }
                        }
                    }

                }
                Err(FsError::EntryNotFound)=>{return Err(FsError::EntryNotFound);}
                r=>{return r;}
            }
        }
        Err(FsError::EntryNotFound)
    }
}




