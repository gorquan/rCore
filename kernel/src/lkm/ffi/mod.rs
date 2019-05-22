use crate::rcore_fs::vfs::{Metadata, Timespec, FileType, PollStatus, FsInfo};
use crate::rcore_fs::vfs::{FsError};
pub mod file_operations;

pub trait PrimitiveCast<P>{
    fn fromPrimitive(p: &P)->Self;
    fn toPrimitive(&self)->P;
}
#[repr(C)]
pub struct MetadataFFI {
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
    pub atime: TimespecFFI,
    /// Time of last modification
    pub mtime: TimespecFFI,
    /// Time of last change
    pub ctime: TimespecFFI,
    /// Type of file
    pub type_: usize,
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
    // Currently we use two u32 to store the rdev on sfs.
    pub rdev: u64
}

#[repr(C)]
pub struct TimespecFFI{
    pub sec: i64,
    pub nsec: i32,
}
impl PrimitiveCast<TimespecFFI> for Timespec{
    fn fromPrimitive(p: &TimespecFFI) -> Self {
        Timespec{
            sec: p.sec,
            nsec: p.nsec
        }
    }

    fn toPrimitive(&self) -> TimespecFFI {
        TimespecFFI{
            sec: self.sec,
            nsec: self.nsec
        }
    }
}

#[repr(C)]
pub struct FsInfoFFI{
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
impl PrimitiveCast<FsInfoFFI> for FsInfo{
    fn fromPrimitive(p: &FsInfoFFI) -> Self {
        FsInfo{
            bsize:p.bsize,
            frsize:p.frsize,
            blocks:p.blocks,
            bfree:p.bfree,
            bavail:p.bavail,
            files:p.files,
            ffree:p.ffree,
            namemax:p.namemax
        }
    }

    fn toPrimitive(&self) -> FsInfoFFI {
        let p=self;
        FsInfoFFI{
            bsize:p.bsize,
            frsize:p.frsize,
            blocks:p.blocks,
            bfree:p.bfree,
            bavail:p.bavail,
            files:p.files,
            ffree:p.ffree,
            namemax:p.namemax
        }
    }
}
impl PrimitiveCast<usize> for FileType{
    fn fromPrimitive(p: &usize) -> Self {
        match p{
            0=>FileType::File,
            1=>FileType::Dir,
            2=>FileType::SymLink,
            3=>FileType::CharDevice,
            4=>FileType::BlockDevice,
            5=>FileType::NamedPipe,
            6=>FileType::Socket,
            _=>panic!("Bad file type!")
        }
    }

    fn toPrimitive(&self) -> usize {
        match self {
            FileType::File=>0,
            FileType::Dir=>1,
            FileType::SymLink=>2,
            FileType::CharDevice=>3,
            FileType::BlockDevice=>4,
            FileType::NamedPipe=>5,
            FileType::Socket=>6
        }
    }
}
impl PrimitiveCast<MetadataFFI> for Metadata{
    fn fromPrimitive(p: &MetadataFFI) -> Self {
        info!("file {} {} {} {} {} {} {} {} {} {} {}", p.dev, p.inode, p.size, p.blk_size, p.blocks, p.type_, p.mode, p.nlinks, p.uid, p.gid, p.rdev);
        Metadata{
            dev: p.dev,
            inode: p.inode,
            size: p.size,
            blk_size: p.blk_size,
            blocks: p.blocks,
            atime: Timespec::fromPrimitive(&p.atime),
            mtime: Timespec::fromPrimitive(&p.mtime),
            ctime: Timespec::fromPrimitive(&p.ctime),
            type_: FileType::fromPrimitive(&p.type_),
            mode: p.mode,
            nlinks: p.nlinks,
            uid: p.uid,
            gid: p.gid,
            rdev: p.rdev
        }
    }

    fn toPrimitive(&self) -> MetadataFFI {
        let p=self;
        MetadataFFI{
            dev: p.dev,
            inode: p.inode,
            size: p.size,
            blk_size: p.blk_size,
            blocks: p.blocks,
            atime: Timespec::toPrimitive(&p.atime),
            mtime: Timespec::toPrimitive(&p.mtime),
            ctime: Timespec::toPrimitive(&p.ctime),
            type_: FileType::toPrimitive(&p.type_),
            mode: p.mode,
            nlinks: p.nlinks,
            uid: p.uid,
            gid: p.gid,
            rdev: p.rdev
        }
    }
}

#[repr(C)]
pub struct PollStatusFFI {
    pub tag_errorwriteread: u8,
}

impl PrimitiveCast<PollStatusFFI> for PollStatus{
    fn fromPrimitive(p: &PollStatusFFI) -> Self {
        PollStatus{
            read: p.tag_errorwriteread&1>0,
            write: p.tag_errorwriteread&2>0,
            error: p.tag_errorwriteread&4>0
        }
    }

    fn toPrimitive(&self) -> PollStatusFFI {
        let p=self;
        let mut flag:u8=0;
        if p.read {flag+=1;}
        if p.write {flag+=2;}
        if p.error {flag+=4;}
        PollStatusFFI{
            tag_errorwriteread:flag
        }
    }
}
fn patch_isize_to_error(s:isize)->FsError{
    match s {
        -1=>FsError::NotSupported,  //E_UNIMP, or E_INVAL
        -2=>FsError::NotFile,       //E_ISDIR
        -3=>FsError::IsDir,         //E_ISDIR, used only in link
        -4=>FsError::NotDir,        //E_NOTDIR
        -5=>FsError::EntryNotFound, //E_NOENT
        -6=>FsError::EntryExist,    //E_EXIST
        -7=>FsError::NotSameFs,     //E_XDEV
        -8=>FsError::InvalidParam,  //E_INVAL
        -9=>FsError::NoDeviceSpace, //E_NOSPC, but is defined and not used in the original ucore, which uses E_NO_MEM
        -10=>FsError::DirRemoved,    //E_NOENT, when the current dir was remove by a previous unlink
        -11=>FsError::DirNotEmpty,   //E_NOTEMPTY
        -12=>FsError::WrongFs,       //E_INVAL, when we find the content on disk is wrong when opening the device
        -13=>FsError::DeviceError,
        -14=>FsError::SymLoop,        //E_LOOP, too many symlink follows.
        -15=>FsError::NoDevice, //E_NXIO
        _=>FsError::NotSupported
    }
}
pub fn patch_isize_to_usize(s: isize)->Result<usize, FsError>{
    if s<0{
        Err(patch_isize_to_error(s))
    }else{
        Ok(s as usize)
    }
}
pub fn patch_i64_to_u64(s: i64)->Result<u64, FsError>{
    if s<0{
        Err(patch_isize_to_error(s as isize))
    }else{
        Ok(s as u64)
    }
}
pub fn patch_isize_to_empty(s: isize)->Result<(), FsError>{
    if s==0{
        Ok(())
    }else{
        Err(patch_isize_to_error(s))
    }
}