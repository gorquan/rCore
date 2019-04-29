use crate::rcore_fs::vfs::{INode, Metadata, Result, INodeContainer};
use alloc::{string::String, sync::Arc};

pub struct File {
    //inode: Arc<INode>,
    inode_container: Arc<INodeContainer>,
    offset: usize,
    readable: bool,
    writable: bool,
}

impl File {
    pub fn new(inode_container: Arc<INodeContainer>, readable: bool, writable: bool) -> Self {
        File {
            inode_container,
            offset: 0,
            readable,
            writable,
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        assert!(self.readable);
        let len = self.inode_container.inode.read_at(self.offset, buf)?;
        self.offset += len;
        Ok(len)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        assert!(self.writable);
        let len = self.inode_container.inode.write_at(self.offset, buf)?;
        self.offset += len;
        Ok(len)
    }

    pub fn info(&self) -> Result<Metadata> {
        self.inode_container.inode.metadata()
    }

    pub fn get_entry(&self, id: usize) -> Result<String> {
        self.inode_container.inode.get_entry(id)
    }
}
