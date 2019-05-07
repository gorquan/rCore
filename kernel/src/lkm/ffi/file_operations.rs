/*
pub struct FileOperations {
    pub open: Option<extern "C" fn()->usize>,
    pub read: Option<extern "C" fn(file: usize, buf: &mut [u8]) -> Result<usize>>,
    pub read_at: Option<extern "C" fn(file: usize, offset: usize, buf: &mut [u8]) -> Result<usize>>,
    pub write: Option<extern "C" fn(file: usize, buf: &[u8]) -> Result<usize>>,
    pub write_at: Option<extern "C" fn(file: usize, offset: usize, buf: &[u8]) -> Result<usize>>,
    pub seek: Option<extern "C" fn(file: usize, pos: SeekFrom) -> Result<u64>>,
    pub set_len: Option<extern "C" fn(file: usize, len: u64) -> Result<()>>,
    pub sync_all: Option<extern "C" fn(file: usize) -> Result<()>>,
    pub sync_data: Option<extern "C" fn(file: usize) -> Result<()>>,
    pub metadata: Option<extern "C" fn(file: usize) -> Result<Metadata>>,
    pub read_entry: Option<extern "C" fn(file: usize) -> Result<String>>,
    pub poll: Option<extern "C" fn (file: usize) -> Result<PollStatus>>,
    pub io_control: Option<extern "C" fn(file: usize, cmd: u32, data: usize) -> Result<()>>,
    pub close: Option<extern "C" fn(file: usize)>
}
*/
use crate::fs::{FileHandle, SeekFrom};
use crate::lkm::cdev::{CDevManager, CharDev, FileOperations};
use crate::rcore_fs::vfs::{FsError, Metadata, PollStatus};
use alloc::string::String;
use alloc::sync::Arc;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct FileOperationsFFI {
    pub open: extern "C" fn() -> usize,
    pub read: extern "C" fn(file: usize, buf: *mut u8, len: usize) -> isize,
    pub read_at: extern "C" fn(file: usize, offset: usize, buf: *mut u8, len: usize) -> isize,
    pub write: extern "C" fn(file: usize, buf: *const u8, len: usize) -> isize,
    pub write_at: extern "C" fn(file: usize, offset: usize, buf: *const u8, len: usize) -> isize,
    pub seek: extern "C" fn(file: usize, pos_mode: usize, pos: usize) -> i64,
    pub set_len: extern "C" fn(file: usize, len: u64) -> isize,
    pub sync_all: extern "C" fn(file: usize) -> isize,
    pub sync_data: extern "C" fn(file: usize) -> isize,
    //pub metadata: extern "C" fn(file: usize) -> isize,
    //pub read_entry: extern "C" fn(file: usize) -> isize,
    pub poll: extern "C" fn(file: usize) -> isize,
    pub io_control: extern "C" fn(file: usize, cmd: u32, data: usize) -> isize,
    pub close: extern "C" fn(file: usize),
}
#[repr(C)]
pub struct CharDevFFI {
    parent_module: usize,
    file_operations_ffi: usize,
    major: u32,
}
#[no_mangle]
pub extern "C" fn lkm_api_register_device(config: *const CharDevFFI) -> usize {
    let config = unsafe { &*config };
    let cdev: CharDev = CharDev {
        parent_module: Some(crate::lkm::api::get_module(config.parent_module).grab()),
        file_op: Arc::new(
            unsafe { &*(config.file_operations_ffi as *const FileOperationsFFI) }.clone(),
        ),
    };
    CDevManager::get()
        .write()
        .registerDevice(config.major, cdev);
    0
}

fn patch_isize_to_usize(s: isize) -> Result<usize, FsError> {
    if s < 0 {
        Err(FsError::NotSupported)
    } else {
        Ok(s as usize)
    }
}
fn patch_i64_to_u64(s: i64) -> Result<u64, FsError> {
    if s < 0 {
        Err(FsError::NotSupported)
    } else {
        Ok(s as u64)
    }
}
fn patch_isize_to_empty(s: isize) -> Result<(), FsError> {
    if s == 0 {
        Ok(())
    } else {
        Err(FsError::NotSupported)
    }
}
impl FileOperations for FileOperationsFFI {
    fn open(&self) -> usize {
        (self.open)()
    }

    fn read(&self, fh: &mut FileHandle, buf: &mut [u8]) -> Result<usize, FsError> {
        patch_isize_to_usize((self.read)(fh.user_data, buf.as_mut_ptr(), buf.len()))
    }

    fn read_at(
        &self,
        fh: &mut FileHandle,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<usize, FsError> {
        patch_isize_to_usize((self.read_at)(
            fh.user_data,
            offset,
            buf.as_mut_ptr(),
            buf.len(),
        ))
    }

    fn write(&self, fh: &mut FileHandle, buf: &[u8]) -> Result<usize, FsError> {
        patch_isize_to_usize((self.write)(fh.user_data, buf.as_ptr(), buf.len()))
    }

    fn write_at(&self, fh: &mut FileHandle, offset: usize, buf: &[u8]) -> Result<usize, FsError> {
        patch_isize_to_usize((self.write_at)(
            fh.user_data,
            offset,
            buf.as_ptr(),
            buf.len(),
        ))
    }

    fn seek(&self, fh: &mut FileHandle, pos: SeekFrom) -> Result<u64, FsError> {
        let (pos_mode, pos) = match pos {
            SeekFrom::Current(pos) => (0 as usize, pos as usize),
            SeekFrom::Start(pos) => (1 as usize, pos as usize),
            SeekFrom::End(pos) => (2 as usize, pos as usize),
        } as (usize, usize);
        patch_i64_to_u64((self.seek)(fh.user_data, pos_mode, pos))
    }

    fn set_len(&self, fh: &mut FileHandle, len: u64) -> Result<(), FsError> {
        patch_isize_to_empty((self.set_len)(fh.user_data, len))
    }

    fn sync_all(&self, fh: &mut FileHandle) -> Result<(), FsError> {
        patch_isize_to_empty((self.sync_all)(fh.user_data))
    }

    fn sync_data(&self, fh: &mut FileHandle) -> Result<(), FsError> {
        patch_isize_to_empty((self.sync_data)(fh.user_data))
    }

    fn metadata(&self, fh: &FileHandle) -> Result<Metadata, FsError> {
        fh.inode_container.inode.metadata()
    }

    fn read_entry(&self, fh: &mut FileHandle) -> Result<String, FsError> {
        Err(FsError::NotDir)
    }

    fn poll(&self, fh: &FileHandle) -> Result<PollStatus, FsError> {
        Err(FsError::NotSupported) //TODO: Important!
    }

    fn io_control(&self, fh: &FileHandle, cmd: u32, arg: usize) -> Result<(), FsError> {
        patch_isize_to_empty((self.io_control)(fh.user_data, cmd, arg))
    }

    fn close(&self, data: usize) {
        (self.close)(data)
    }
}
