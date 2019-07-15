//! File handle for process

use crate::thread;
use alloc::{string::String, sync::Arc};
use core::fmt;

use rcore_fs_mountfs::MNode as INodeContainer;
use rcore_fs::vfs::{FsError, INode, Metadata, PollStatus, Result};

#[derive(Clone)]
pub struct FileHandle {
    pub inode_container: Arc<INodeContainer>,
    offset: u64,
    options: OpenOptions,
}

#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    /// Before each write, the file offset is positioned at the end of the file.
    pub append: bool,
    pub nonblock: bool,
}

#[derive(Debug)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

impl FileHandle {
    pub fn new(inode_container: Arc<INodeContainer>, options: OpenOptions) -> Self {
        return FileHandle {
            inode_container,
            offset: 0,
            options,
        };
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let len = self.read_at(self.offset as usize, buf)?;
        self.offset += len as u64;
        Ok(len)
    }

    pub fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        if !self.options.read {
            return Err(FsError::InvalidParam); // FIXME: => EBADF
        }
        let mut len: usize = 0;
        if !self.options.nonblock {
            // block
            loop {
                match self.inode_container.read_at(offset, buf) {
                    Ok(read_len) => {
                        len = read_len;
                        break;
                    }
                    Err(FsError::Again) => {
                        thread::yield_now();
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
        } else {
            len = self.inode_container.read_at(offset, buf)?;
        }
        Ok(len)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let offset = match self.options.append {
            true => self.inode_container.metadata()?.size as u64,
            false => self.offset,
        } as usize;
        let len = self.write_at(offset, buf)?;
        self.offset = (offset + len) as u64;
        Ok(len)
    }

    pub fn write_at(&mut self, offset: usize, buf: &[u8]) -> Result<usize> {
        if !self.options.write {
            return Err(FsError::InvalidParam); // FIXME: => EBADF
        }
        let len = self.inode_container.write_at(offset, buf)?;
        Ok(len)
    }

    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        self.offset = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.inode_container.metadata()?.size as i64 + offset) as u64,
            SeekFrom::Current(offset) => (self.offset as i64 + offset) as u64,
        };
        Ok(self.offset)
    }

    pub fn set_len(&mut self, len: u64) -> Result<()> {
        if !self.options.write {
            return Err(FsError::InvalidParam); // FIXME: => EBADF
        }
        self.inode_container.resize(len as usize)?;
        Ok(())
    }

    pub fn sync_all(&mut self) -> Result<()> {
        self.inode_container.sync_all()
    }

    pub fn sync_data(&mut self) -> Result<()> {
        self.inode_container.sync_data()
    }

    pub fn metadata(&self) -> Result<Metadata> {
        self.inode_container.metadata()
    }

    pub fn read_entry(&mut self) -> Result<String> {
        if !self.options.read {
            return Err(FsError::InvalidParam); // FIXME: => EBADF
        }
        let name = self.inode_container.get_entry(self.offset as usize)?;
        self.offset += 1;
        Ok(name)
    }

    pub fn poll(&self) -> Result<PollStatus> {
        self.inode_container.poll()
    }

    pub fn io_control(&self, cmd: u32, arg: usize) -> Result<()> {
        self.inode_container.io_control(cmd, arg)
    }

    pub fn inode(&self) -> Arc<INodeContainer> {
        self.inode_container.clone()
    }

    pub fn fcntl(&mut self, cmd: usize, arg: usize) -> Result<()> {
        if arg == 2048 && cmd == 4 {
            self.options.nonblock = true;
        }
        Ok(())
    }
}

impl fmt::Debug for FileHandle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        return f
            .debug_struct("FileHandle")
            .field("offset", &self.offset)
            .field("options", &self.options)
            .finish();
    }
}
