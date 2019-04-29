use crate::rcore_fs::*;
use crate::rcore_fs::vfs::{INode, FileSystem, FsError, Metadata, FsInfo, FileType};
use alloc::sync::Arc;
use alloc::string::String;
use core::any::Any;
use core::cell::RefCell;
/*
// The point of designing filesystem is that filesystem just need to make sure that itself is correct.
// If you mess up with your files, that is your fault.
pub struct TempFS{
    root: Arc<TempFSNode>,
    inode_counter: usize
}

enum TempFSNodeContent{
    Folder{
        parent: TempFSNode
    },
    File{

    }
}
pub struct TempFSNode{
    metadata: RefCell<Metadata>,
    data: TempFSNodeContent
}
impl FileSystem for TempFS{
    fn sync(&self) -> Result<(), FsError> {
        // no need to synchronize, since tmpfs is tmpfs.
        Ok(())
    }

    fn root_inode(&self) -> Arc<INode> {
        Arc::clone(&self.root)
    }

    fn info(&self) -> FsInfo {
        FsInfo{
            bsize: 0,
            frsize: 0,
            blocks: 0,
            bfree: 0,
            bavail: 0,
            files: 0,
            ffree: 0,
            namemax: 256
        }
    }
}

impl INode for TempFSNode{
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, FsError> {
        unimplemented!()
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, FsError> {
        unimplemented!()
    }

    fn metadata(&self) -> Result<Metadata, FsError> {
        Ok(self.metadata.get_mut().clone())
    }

    fn chmod(&self, mode: u16) -> Result<(), FsError> {
        self.metadata.mode
    }

    fn sync_all(&self) -> Result<(), FsError> {
        Ok(())
    }

    fn sync_data(&self) -> Result<(), FsError> {
        Ok(())
    }

    fn resize(&self, len: usize) -> Result<(), FsError> {
        unimplemented!()
    }

    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>, FsError> {
        unimplemented!()
    }

    fn unlink(&self, name: &str) -> Result<(), FsError> {
        unimplemented!()
    }

    fn link(&self, name: &str, other: &Arc<INode>) -> Result<(), FsError> {
        unimplemented!()
    }

    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<(), FsError> {
        unimplemented!()
    }

    fn find(&self, name: &str) -> Result<Arc<INode>, FsError> {
        unimplemented!()
    }

    fn get_entry(&self, id: usize) -> Result<String, FsError> {
        unimplemented!()
    }

    fn fs(&self) -> Arc<FileSystem> {
        unimplemented!()
    }

    fn as_any_ref(&self) -> &Any {
        unimplemented!()
    }
}
*/