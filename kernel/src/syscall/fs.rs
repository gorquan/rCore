//! Syscalls for file system

use core::cell::UnsafeCell;
use core::cmp::min;
use core::mem::size_of;

use crate::fs::vfs::{PathConfig, PathResolveResult};
#[cfg(not(target_arch = "mips"))]
use rcore_fs::vfs::Timespec;
use rcore_fs_mountfs::MNode as INodeContainer;

use crate::drivers::SOCKET_ACTIVITY;
use crate::fs::*;
use crate::memory::MemorySet;
use crate::sync::Condvar;

use bitvec::prelude::{BitSlice, BitVec, LittleEndian};

use super::*;
use xmas_elf::dynamic::Tag::SymTabShIndex;

use alloc::slice::SliceConcatExt;
use alloc::sync::Weak;
use spin::RwLock;

impl Syscall<'_> {
    pub fn sys_read(&mut self, fd: usize, base: *mut u8, len: usize) -> SysResult {
        let mut proc = self.process();
        if !proc.pid.is_init() {
            // we trust pid 0 process
            info!("read: fd: {}, base: {:?}, len: {:#x}", fd, base, len);
        }
        let slice = unsafe { self.vm().check_write_array(base, len)? };
        let file_like = proc.get_file_like(fd)?;
        let len = file_like.read(slice)?;
        Ok(len)
    }

    pub fn sys_write(&mut self, fd: usize, base: *const u8, len: usize) -> SysResult {
        let mut proc = self.process();
        if !proc.pid.is_init() {
            // we trust pid 0 process
            info!("write: fd: {}, base: {:?}, len: {:#x}", fd, base, len);
        }
        let slice = unsafe { self.vm().check_read_array(base, len)? };
        let file_like = proc.get_file_like(fd)?;
        let len = file_like.write(slice)?;
        Ok(len)
    }

    pub fn sys_pread(&mut self, fd: usize, base: *mut u8, len: usize, offset: usize) -> SysResult {
        info!(
            "pread: fd: {}, base: {:?}, len: {}, offset: {}",
            fd, base, len, offset
        );
        let mut proc = self.process();
        let slice = unsafe { self.vm().check_write_array(base, len)? };
        let len = proc.get_file(fd)?.read_at(offset, slice)?;
        Ok(len)
    }

    pub fn sys_pwrite(
        &mut self,
        fd: usize,
        base: *const u8,
        len: usize,
        offset: usize,
    ) -> SysResult {
        info!(
            "pwrite: fd: {}, base: {:?}, len: {}, offset: {}",
            fd, base, len, offset
        );
        let mut proc = self.process();
        let slice = unsafe { self.vm().check_read_array(base, len)? };
        let len = proc.get_file(fd)?.write_at(offset, slice)?;
        Ok(len)
    }

    pub fn sys_ppoll(
        &mut self,
        ufds: *mut PollFd,
        nfds: usize,
        timeout: *const TimeSpec,
    ) -> SysResult {
        let proc = self.process();
        let timeout_msecs = if timeout.is_null() {
            1 << 31 // infinity
        } else {
            let timeout = unsafe { self.vm().check_read_ptr(timeout)? };
            timeout.to_msec()
        };
        drop(proc);

        self.sys_poll(ufds, nfds, timeout_msecs as usize)
    }

    pub fn sys_poll(&mut self, ufds: *mut PollFd, nfds: usize, timeout_msecs: usize) -> SysResult {
        let proc = self.process();
        if !proc.pid.is_init() {
            // we trust pid 0 process
            info!(
                "poll: ufds: {:?}, nfds: {}, timeout_msecs: {:#x}",
                ufds, nfds, timeout_msecs
            );
        }
        let polls = unsafe { self.vm().check_write_array(ufds, nfds)? };
        for poll in polls.iter() {
            if proc.files.get(&(poll.fd as usize)).is_none() {
                return Err(SysError::EINVAL);
            }
        }
        drop(proc);

        let begin_time_ms = crate::trap::uptime_msec();
        Condvar::wait_events(&[&STDIN_INODE.pushed, &(*SOCKET_ACTIVITY)], move || {
            use PollEvents as PE;
            let proc = self.process();
            let mut events = 0;
            for poll in polls.iter_mut() {
                poll.revents = PE::empty();
                if let Some(file_like) = proc.files.get(&(poll.fd as usize)) {
                    let status = match file_like.poll() {
                        Ok(ret) => ret,
                        Err(err) => return Some(Err(err)),
                    };
                    if status.error {
                        poll.revents |= PE::HUP;
                        events += 1;
                    }
                    if status.read && poll.events.contains(PE::IN) {
                        poll.revents |= PE::IN;
                        events += 1;
                    }
                    if status.write && poll.events.contains(PE::OUT) {
                        poll.revents |= PE::OUT;
                        events += 1;
                    }
                } else {
                    poll.revents |= PE::ERR;
                    events += 1;
                }
            }
            drop(proc);

            if events > 0 {
                return Some(Ok(events));
            }

            let current_time_ms = crate::trap::uptime_msec();
            if timeout_msecs < (1 << 31) && current_time_ms - begin_time_ms > timeout_msecs {
                return Some(Ok(0));
            }
            return None;
        })
    }

    pub fn sys_select(
        &mut self,
        nfds: usize,
        read: *mut u32,
        write: *mut u32,
        err: *mut u32,
        timeout: *const TimeVal,
    ) -> SysResult {
        info!(
            "select: nfds: {}, read: {:?}, write: {:?}, err: {:?}, timeout: {:?}",
            nfds, read, write, err, timeout
        );

        let proc = self.process();
        let mut read_fds = FdSet::new(&self.vm(), read, nfds)?;
        let mut write_fds = FdSet::new(&self.vm(), write, nfds)?;
        let mut err_fds = FdSet::new(&self.vm(), err, nfds)?;
        let timeout_msecs = if !timeout.is_null() {
            let timeout = unsafe { self.vm().check_read_ptr(timeout)? };
            timeout.to_msec()
        } else {
            // infinity
            1 << 31
        };
        // for debugging
        if cfg!(debug_assertions) {
            debug!("files before select {:#?}", proc.files);
        }
        drop(proc);

        let begin_time_ms = crate::trap::uptime_msec();

        Condvar::wait_events(&[&STDIN_INODE.pushed, &(*SOCKET_ACTIVITY)], move || {
            let proc = self.process();
            let mut events = 0;
            for (&fd, file_like) in proc.files.iter() {
                if fd >= nfds {
                    continue;
                }
                if !err_fds.contains(fd) && !read_fds.contains(fd) && !write_fds.contains(fd) {
                    continue;
                }
                let status = match file_like.poll() {
                    Ok(ret) => ret,
                    Err(err) => return Some(Err(err)),
                };
                if status.error && err_fds.contains(fd) {
                    err_fds.set(fd);
                    events += 1;
                }
                if status.read && read_fds.contains(fd) {
                    read_fds.set(fd);
                    events += 1;
                }
                if status.write && write_fds.contains(fd) {
                    write_fds.set(fd);
                    events += 1;
                }
            }
            drop(proc);

            if events > 0 {
                return Some(Ok(events));
            }

            if timeout_msecs == 0 {
                // no timeout, return now;
                return Some(Ok(0));
            }

            let current_time_ms = crate::trap::uptime_msec();
            // infinity check
            if timeout_msecs < (1 << 31) && current_time_ms - begin_time_ms > timeout_msecs as usize
            {
                return Some(Ok(0));
            }

            return None;
        })
    }

    pub fn sys_readv(&mut self, fd: usize, iov_ptr: *const IoVec, iov_count: usize) -> SysResult {
        info!(
            "readv: fd: {}, iov: {:?}, count: {}",
            fd, iov_ptr, iov_count
        );
        let mut proc = self.process();
        let mut iovs = unsafe { IoVecs::check_and_new(iov_ptr, iov_count, &self.vm(), true)? };

        // read all data to a buf
        let file_like = proc.get_file_like(fd)?;
        let mut buf = iovs.new_buf(true);
        let len = file_like.read(buf.as_mut_slice())?;
        // copy data to user
        iovs.write_all_from_slice(&buf[..len]);
        Ok(len)
    }

    pub fn sys_writev(&mut self, fd: usize, iov_ptr: *const IoVec, iov_count: usize) -> SysResult {
        let mut proc = self.process();
        if !proc.pid.is_init() {
            // we trust pid 0 process
            info!(
                "writev: fd: {}, iov: {:?}, count: {}",
                fd, iov_ptr, iov_count
            );
        }
        let iovs = unsafe { IoVecs::check_and_new(iov_ptr, iov_count, &self.vm(), false)? };

        let buf = iovs.read_all_to_vec();
        let file_like = proc.get_file_like(fd)?;
        let len = file_like.write(buf.as_slice())?;
        Ok(len)
    }

    pub fn sys_open(&mut self, path: *const u8, flags: usize, mode: usize) -> SysResult {
        self.sys_openat(AT_FDCWD, path, flags, mode)
    }

    pub fn sys_openat(
        &mut self,
        dir_fd: usize,
        path: *const u8,
        flags: usize,
        mode: usize,
    ) -> SysResult {
        let mut proc = self.process();
        let path = unsafe { check_and_clone_cstr(path)? };
        let flags = OpenFlags::from_bits_truncate(flags);
        info!(
            "openat: dir_fd: {}, path: {:?}, flags: {:?}, mode: {:#o}",
            dir_fd as isize, path, flags, mode
        );
        let start_directory = Arc::clone(if dir_fd == AT_FDCWD {
            &proc.cwd.cwd
        } else {
            &proc.get_file(dir_fd)?.inode_container
        });
        let resolve_result = proc.cwd.path_resolve(&start_directory, &path, true)?;
        let ic = match resolve_result {
            PathResolveResult::IsFile { file, parent, .. } => {
                if flags.contains(OpenFlags::EXCLUSIVE) {
                    return Err(SysError::EEXIST);
                }
                file
            }
            PathResolveResult::IsDir { dir } => {
                if flags.contains(OpenFlags::EXCLUSIVE) {
                    return Err(SysError::EEXIST);
                }
                dir
            }
            PathResolveResult::NotExist { parent, name } => {
                if flags.contains(OpenFlags::CREATE) {
                    parent.create(&name, FileType::File, mode as u32)?
                } else {
                    return Err(SysError::ENOENT);
                }
            }
        };

        let file = if ic.metadata()?.type_ == FileType::CharDevice {
            panic!("Device file not supported!");
        } else {
            FileLike::File(FileHandle::new(ic, flags.to_options()))
        };
        let fd = proc.add_file(file);
        Ok(fd)
    }

    pub fn sys_close(&mut self, fd: usize) -> SysResult {
        info!("close: fd: {:?}", fd);
        let mut proc = self.process();

        // for debugging
        if cfg!(debug_assertions) {
            debug!("files before close {:#?}", proc.files);
        }

        proc.files.remove(&fd).ok_or(SysError::EBADF)?;
        Ok(0)
    }

    pub fn sys_access(&mut self, path: *const u8, mode: usize) -> SysResult {
        self.sys_faccessat(AT_FDCWD, path, mode, 0)
    }

    pub fn sys_faccessat(
        &mut self,
        dirfd: usize,
        path: *const u8,
        mode: usize,
        flags: usize,
    ) -> SysResult {
        // TODO: check permissions based on uid/gid
        let proc = self.process();
        let path = unsafe { check_and_clone_cstr(path)? };
        let flags = AtFlags::from_bits_truncate(flags);
        if !proc.pid.is_init() {
            // we trust pid 0 process
            info!(
                "faccessat: dirfd: {}, path: {:?}, mode: {:#o}, flags: {:?}",
                dirfd as isize, path, mode, flags
            );
        }

        let result = proc.cwd.path_resolve(&proc.cwd.cwd, &path, true)?;
        if let PathResolveResult::NotExist { .. } = result {
            return Err(SysError::ENOENT);
        }

        Ok(0)
    }

    pub fn sys_getcwd(&mut self, buf: *mut u8, len: usize) -> SysResult {
        let proc = self.process();
        if !proc.pid.is_init() {
            // we trust pid 0 process
            info!("getcwd: buf: {:?}, len: {:#x}", buf, len);
        }
        let buf = unsafe { self.vm().check_write_array(buf, len)? };

        // TODO: a more graceful and natural implementation?
        let mut current_inode = Arc::clone(&proc.cwd.cwd);
        let root_inode_id = proc.cwd.root.metadata().unwrap().inode;
        let total_root_vfs: Arc<INodeContainer> = VIRTUAL_FS.root_inode();
        let total_inode_id = total_root_vfs.metadata().unwrap().inode;
        let mut path_parts: Vec<String> = Vec::new();
        let mut unreachable = false;
        loop {
            let current_inode_id = current_inode.metadata().unwrap().inode;
            if Arc::ptr_eq(&current_inode.vfs, &proc.cwd.root.vfs)
                && current_inode_id == root_inode_id
            {
                //Reaching our root;
                //Reaching our root
                break;
            } else if Arc::ptr_eq(&total_root_vfs.vfs, &proc.cwd.root.vfs)
                && current_inode_id == total_inode_id
            {
                //Reaching total root before our root.
                //Reaching total root
                unreachable = true;
                break;
            } else {
                //Safe to go down a stair.
                let parent = unsafe { proc.cwd.force_resolve_parent(&current_inode) };
                let name = parent.find_name_by_child(&current_inode)?;
                path_parts.push(name);
                current_inode = parent;
            }
            // The stairs will finish, and root will be reached.
        }

        path_parts.reverse();
        let total_string = if unreachable {
            String::from("(unreachable)")
        } else {
            String::from("/") + &path_parts.join("/")
        };
        if total_string.len() + 1 > len {
            return Err(SysError::ERANGE);
        }

        unsafe { util::write_cstr(buf.as_mut_ptr(), &total_string) }
        Ok(buf.as_ptr() as usize)
    }

    fn impl_sys_stat(
        &mut self,
        start_directory: Arc<INodeContainer>,
        path: *const u8,
        stat_ptr: *mut Stat,
        resolve_link: bool,
    ) -> SysResult {
        let proc = self.process();

        let path = unsafe { check_and_clone_cstr(path)? };

        let stat_ref = unsafe { self.vm().check_write_ptr(stat_ptr)? };

        let inode = (match proc
            .cwd
            .path_resolve(&start_directory, &path, resolve_link)?
        {
            PathResolveResult::IsDir { dir } => dir,
            PathResolveResult::IsFile { file, .. } => file,
            PathResolveResult::NotExist { .. } => {
                return Err(SysError::ENOENT);
            }
        });

        let stat = Stat::from(inode.metadata()?);
        *stat_ref = stat;
        Ok(0)
    }
    pub fn sys_stat(&mut self, path: *const u8, stat_ptr: *mut Stat) -> SysResult {
        let proc = self.process();

        info!("stat: path: {:?}, stat_ptr: {:?}", path, stat_ptr);

        let cwd = Arc::clone(&proc.cwd.cwd);
        drop(proc);
        self.impl_sys_stat(cwd, path, stat_ptr, true)
    }

    pub fn sys_fstat(&mut self, fd: usize, stat_ptr: *mut Stat) -> SysResult {
        info!("fstat: fd: {}, stat_ptr: {:?}", fd, stat_ptr);
        let mut proc = self.process();
        let stat_ref = unsafe { self.vm().check_write_ptr(stat_ptr)? };
        let file = proc.get_file(fd)?;
        let stat = Stat::from(file.metadata()?);
        *stat_ref = stat;
        Ok(0)
    }

    pub fn sys_lstat(&mut self, path: *const u8, stat_ptr: *mut Stat) -> SysResult {
        info!("lstat: path: {:?}, stat_ptr: {:?}", path, stat_ptr);
        let proc = self.process();
        let cwd = Arc::clone(&proc.cwd.cwd);
        drop(proc);
        self.impl_sys_stat(cwd, path, stat_ptr, false)
    }

    pub fn sys_fstatat(
        &mut self,
        dirfd: usize,
        pathname: *const u8,
        stat_ptr: *mut Stat,
        flags: usize,
    ) -> SysResult {
        let mut proc = self.process();
        let dir = if dirfd == AT_FDCWD {
            Arc::clone(&proc.cwd.cwd)
        } else {
            Arc::clone(&proc.get_file(dirfd)?.inode_container)
        };
        drop(proc);
        self.impl_sys_stat(dir, pathname, stat_ptr, (flags & 0x100) == 0)
    }
    pub fn sys_readlink(&mut self, path: *const u8, base: *mut u8, len: usize) -> SysResult {
        self.sys_readlinkat(AT_FDCWD, path, base, len)
    }

    pub fn sys_readlinkat(
        &mut self,
        dirfd: usize,
        path: *const u8,
        base: *mut u8,
        len: usize,
    ) -> SysResult {
        let proc = self.process();
        let path = unsafe { check_and_clone_cstr(path)? };
        let slice = unsafe { self.vm().check_write_array(base, len)? };
        info!("readlink: path: {:?}, base: {:?}, len: {}", path, base, len);

        let inode = match proc.cwd.path_resolve(&proc.cwd.cwd, &path, false)? {
            PathResolveResult::IsDir { dir } => dir,
            PathResolveResult::IsFile { file, .. } => file,
            PathResolveResult::NotExist { .. } => {
                return Err(SysError::ENOENT);
            }
        };

        if inode.metadata()?.type_ == FileType::SymLink {
            // TODO: recursive link resolution and loop detection
            let mut slice = unsafe { slice::from_raw_parts_mut(base, len) };
            let len = inode.read_at(0, slice)?;
            Ok(len)
        } else {
            Err(SysError::EINVAL)
        }
    }

    pub fn sys_lseek(&mut self, fd: usize, offset: i64, whence: u8) -> SysResult {
        let pos = match whence {
            SEEK_SET => SeekFrom::Start(offset as u64),
            SEEK_END => SeekFrom::End(offset),
            SEEK_CUR => SeekFrom::Current(offset),
            _ => return Err(SysError::EINVAL),
        };
        info!("lseek: fd: {}, pos: {:?}", fd, pos);

        let mut proc = self.process();
        let file = proc.get_file(fd)?;
        let offset = file.seek(pos)?;
        Ok(offset as usize)
    }

    pub fn sys_fsync(&mut self, fd: usize) -> SysResult {
        info!("fsync: fd: {}", fd);
        self.process().get_file(fd)?.sync_all()?;
        Ok(0)
    }

    pub fn sys_fdatasync(&mut self, fd: usize) -> SysResult {
        info!("fdatasync: fd: {}", fd);
        self.process().get_file(fd)?.sync_data()?;
        Ok(0)
    }

    pub fn sys_truncate(&mut self, path: *const u8, len: usize) -> SysResult {
        let proc = self.process();
        let path = unsafe { check_and_clone_cstr(path)? };
        info!("truncate: path: {:?}, len: {}", path, len);
        let inode = (match proc.cwd.path_resolve(&proc.cwd.cwd, &path, true)? {
            PathResolveResult::IsDir { dir } => dir,
            PathResolveResult::IsFile { file, .. } => file,
            PathResolveResult::NotExist { .. } => {
                return Err(SysError::ENOENT);
            }
        });
        inode.resize(len)?;
        Ok(0)
    }

    pub fn sys_ftruncate(&mut self, fd: usize, len: usize) -> SysResult {
        info!("ftruncate: fd: {}, len: {}", fd, len);
        self.process().get_file(fd)?.set_len(len as u64)?;
        Ok(0)
    }

    pub fn sys_getdents64(
        &mut self,
        fd: usize,
        buf: *mut LinuxDirent64,
        buf_size: usize,
    ) -> SysResult {
        info!(
            "getdents64: fd: {}, ptr: {:?}, buf_size: {}",
            fd, buf, buf_size
        );
        let mut proc = self.process();
        let buf = unsafe { self.vm().check_write_array(buf as *mut u8, buf_size)? };
        let file = proc.get_file(fd)?;
        let info = file.metadata()?;
        if info.type_ != FileType::Dir {
            return Err(SysError::ENOTDIR);
        }
        let ic = Arc::clone(&file.inode_container);
        let mut writer = unsafe { DirentBufWriter::new(buf) };
        let is_file_root = proc.cwd.has_reached_root(&ic);
        let is_current_very_root = PathConfig::init_root().has_reached_root(&ic);
        let file = proc.get_file(fd)?; //re-borrow?
        loop {
            let name = match file.read_entry() {
                Err(FsError::EntryNotFound) => break,
                r => r,
            }?;
            // TODO: get ino from dirent

            let target = ic.find(is_file_root || is_current_very_root, &name)?;
            let info = target.metadata()?;
            let ok = writer.try_write(
                info.inode as u64,
                DirentType::from_type(&info.type_).bits(),
                &name,
            );
            if !ok {
                break;
            }
        }
        Ok(writer.written_size)
    }

    pub fn sys_dup2(&mut self, fd1: usize, fd2: usize) -> SysResult {
        info!("dup2: from {} to {}", fd1, fd2);
        let mut proc = self.process();
        // close fd2 first if it is opened
        proc.files.remove(&fd2);

        let file_like = proc.get_file_like(fd1)?.clone();
        proc.files.insert(fd2, file_like);
        Ok(fd2)
    }

    pub fn sys_ioctl(
        &mut self,
        fd: usize,
        request: usize,
        arg1: usize,
        arg2: usize,
        arg3: usize,
    ) -> SysResult {
        info!(
            "ioctl: fd: {}, request: {:#x}, args: {:#x} {:#x} {:#x}",
            fd, request, arg1, arg2, arg3
        );
        let mut proc = self.process();
        let file_like = proc.get_file_like(fd)?;
        file_like.ioctl(request, arg1, arg2, arg3)
    }

    pub fn sys_chdir(&mut self, path: *const u8) -> SysResult {
        let mut proc = self.process();
        let path = check_and_clone_cstr(path)?;
        if !proc.pid.is_init() {
            // we trust pid 0 process
            info!("chdir: path: {:?}", path);
        }

        let inode = proc.cwd.path_resolve(&proc.cwd.cwd, &path, true)?;
        if let PathResolveResult::IsDir { dir } = inode {
            proc.cwd.cwd = dir;
            Ok(0)
        } else {
            Err(SysError::ENOTDIR)
        }
    }

    pub fn sys_rename(&mut self, oldpath: *const u8, newpath: *const u8) -> SysResult {
        self.sys_renameat(AT_FDCWD, oldpath, AT_FDCWD, newpath)
    }

    pub fn sys_renameat(
        &mut self,
        olddirfd: usize,
        oldpath: *const u8,
        newdirfd: usize,
        newpath: *const u8,
    ) -> SysResult {
        let mut proc = self.process();
        let oldpath = check_and_clone_cstr(oldpath)?;
        let newpath = check_and_clone_cstr(newpath)?;
        info!(
            "renameat: olddirfd: {}, oldpath: {:?}, newdirfd: {}, newpath: {:?}",
            olddirfd as isize, oldpath, newdirfd as isize, newpath
        );
        let old_start_directory = Arc::clone(if olddirfd == AT_FDCWD {
            &proc.cwd.cwd
        } else {
            &proc.get_file(olddirfd)?.inode_container
        });
        let old_file = proc
            .cwd
            .path_resolve(&old_start_directory, &oldpath, false)?;
        let (old_parent, old_name) = match old_file {
            PathResolveResult::IsDir { dir, .. } => {
                let parent = dir.find(proc.cwd.has_reached_root(&dir), "..")?;
                let name = parent.find_name_by_child(&dir)?;
                if name == "." || name == ".." {
                    //How dare you move root?
                    Err(FsError::DirNotEmpty)?;
                }
                (parent, name)
            }
            PathResolveResult::IsFile { name, parent, .. } => (parent, name),
            PathResolveResult::NotExist { .. } => {
                return Err(SysError::ENOENT);
            }
        };
        let new_start_directory = Arc::clone(if newdirfd == AT_FDCWD {
            &proc.cwd.cwd
        } else {
            &proc.get_file(newdirfd)?.inode_container
        });
        let new_file = proc
            .cwd
            .path_resolve(&new_start_directory, &newpath, true)?;
        let (new_parent, new_name) = match new_file {
            PathResolveResult::IsDir { .. } => {
                return Err(SysError::EEXIST);
            }
            PathResolveResult::IsFile { .. } => {
                return Err(SysError::EEXIST);
            }
            PathResolveResult::NotExist { parent, name, .. } => (parent, name),
        };
        if Arc::ptr_eq(&old_parent.vfs, &new_parent.vfs) {
            old_parent
                .move_(&old_name, &(new_parent as Arc<dyn INode>), &new_name)?;
        } else {
            Err(FsError::NotSameFs)?;
        }
        Ok(0)

        //Err(SysError::ENOSYS)
    }

    pub fn sys_mkdir(&mut self, path: *const u8, mode: usize) -> SysResult {
        self.sys_mkdirat(AT_FDCWD, path, mode)
    }

    pub fn sys_mkdirat(&mut self, dirfd: usize, path: *const u8, mode: usize) -> SysResult {
        let mut proc = self.process();
        let path = check_and_clone_cstr(path)?;
        // TODO: check pathname
        info!(
            "mkdirat: dirfd: {}, path: {:?}, mode: {:#o}",
            dirfd as isize, path, mode
        );
        let start_directory = Arc::clone(if dirfd == AT_FDCWD {
            &proc.cwd.cwd
        } else {
            &proc.get_file(dirfd)?.inode_container
        });
        match proc.cwd.path_resolve(&start_directory, &path, false)? {
            PathResolveResult::IsDir { dir } => {
                return Err(SysError::EEXIST);
            }
            PathResolveResult::IsFile { file, .. } => {
                return Err(SysError::EEXIST);
            }
            PathResolveResult::NotExist {
                parent,
                name: file_name,
            } => {
                parent.create(&file_name, FileType::Dir, mode as u32)?;
                return Ok(0);
            }
        }
    }

    pub fn sys_mknodat(
        &self,
        dir_fd: usize,
        path: *const u8,
        mode: usize,
        dev: usize,
    ) -> SysResult {
        let mut proc = self.process();
        let path = unsafe { check_and_clone_cstr(path)? };
        // TODO: check pathname
        info!("mknod: path: {:?}, mode: {:#o}", path, mode);
        let start_directory = Arc::clone(if dir_fd == AT_FDCWD {
            &proc.cwd.cwd
        } else {
            &proc.get_file(dir_fd)?.inode_container
        });
        //let flags=OpenFlags.
        match proc.cwd.path_resolve(&proc.cwd.cwd, &path, false)? {
            PathResolveResult::IsDir { dir } => {
                return Err(SysError::EEXIST);
            }
            PathResolveResult::IsFile { file, .. } => {
                return Err(SysError::EEXIST);
            }
            PathResolveResult::NotExist {
                parent,
                name: file_name,
            } => {
                // TODO: assume creating a CharDevice.
                // To simplify we don't allow BlockDevice here, like FreeBSD.
                // Need also consider named pipe, named socket and so on.

                // TODO: current sfs impl does not allow creating CharDevice file.
                // Fix this.
                let inode = parent.create(&file_name, FileType::CharDevice, mode as u32)?;
                return Ok(0);
            }
        }
    }

    pub fn sys_rmdir(&mut self, path: *const u8) -> SysResult {
        let proc = self.process();
        let path = check_and_clone_cstr(path)?;
        info!("rmdir: path: {:?}", path);

        let inode = match proc.cwd.path_resolve(&proc.cwd.cwd, &path, false)? {
            PathResolveResult::IsDir { dir } => dir,
            PathResolveResult::IsFile { file, .. } => {
                return Err(SysError::ENOTDIR);
            }
            PathResolveResult::NotExist { .. } => {
                return Err(SysError::ENOENT);
            }
        };

        if proc.cwd.has_reached_root(&inode) || PathConfig::init_root().has_reached_root(&inode) {
            return Err(SysError::EBUSY);
        }
        //now safely find parent.
        let parent = inode.find(false, "..")?;
        // inode is not a mountpoint, so two files lie in same fs.
        let name = parent.find_name_by_child(&inode)?;
        parent.unlink(&name)?;
        Ok(0)
    }

    pub fn sys_link(&mut self, oldpath: *const u8, newpath: *const u8) -> SysResult {
        self.sys_linkat(AT_FDCWD, oldpath, AT_FDCWD, newpath, 0)
    }

    pub fn sys_linkat(
        &mut self,
        olddirfd: usize,
        oldpath: *const u8,
        newdirfd: usize,
        newpath: *const u8,
        flags: usize,
    ) -> SysResult {
        let proc = self.process();
        let oldpath = check_and_clone_cstr(oldpath)?;
        let newpath = check_and_clone_cstr(newpath)?;
        let flags = AtFlags::from_bits_truncate(flags);
        info!(
            "linkat: olddirfd: {}, oldpath: {:?}, newdirfd: {}, newpath: {:?}, flags: {:?}",
            olddirfd as isize, oldpath, newdirfd as isize, newpath, flags
        );
        let inode = match proc.cwd.path_resolve(&proc.cwd.cwd, &oldpath, false)? {
            PathResolveResult::IsDir { dir } => {
                return Err(SysError::EISDIR);
            }
            PathResolveResult::IsFile { file, .. } => file,
            PathResolveResult::NotExist { .. } => {
                return Err(SysError::ENOENT);
            }
        };
        match proc.cwd.path_resolve(&proc.cwd.cwd, &newpath, false)? {
            PathResolveResult::NotExist { parent, name } => {
                parent.link(&name, &(inode as Arc<dyn INode>))?;
                Ok(0)
            }
            _ => Err(SysError::EEXIST),
        }
    }

    pub fn sys_unlink(&mut self, path: *const u8) -> SysResult {
        self.sys_unlinkat(AT_FDCWD, path, 0)
    }

    pub fn sys_unlinkat(&mut self, dirfd: usize, path: *const u8, flags: usize) -> SysResult {
        let proc = self.process();
        let path = unsafe { check_and_clone_cstr(path)? };
        let flags = AtFlags::from_bits_truncate(flags);
        info!(
            "unlinkat: dirfd: {}, path: {:?}, flags: {:?}",
            dirfd as isize, path, flags
        );

        match proc.cwd.path_resolve(&proc.cwd.cwd, &path, false)? {
            PathResolveResult::IsDir { .. } => Err(SysError::EISDIR),
            PathResolveResult::NotExist { .. } => Err(SysError::ENOENT),
            PathResolveResult::IsFile { file, parent, .. } => {
                let name = parent.find_name_by_child(&file)?;
                parent.unlink(&name)?;
                Ok(0)
            }
        }
    }

    pub fn sys_pipe(&mut self, fds: *mut u32) -> SysResult {
        info!("pipe: fds: {:?}", fds);

        let mut proc = self.process();
        let mut fds = unsafe { self.vm().check_write_array(fds, 2)? };
        let (read, write) = Pipe::create_pair();

        let read_fd = proc.add_file(FileLike::File(FileHandle::new(
            unsafe { INodeContainer::anonymous_inode(Arc::new(read)) },
            OpenOptions {
                read: true,
                write: false,
                append: false,
                nonblock: false,
            },
        )));

        let write_fd = proc.add_file(FileLike::File(FileHandle::new(
            unsafe { INodeContainer::anonymous_inode(Arc::new(write)) },
            OpenOptions {
                read: false,
                write: true,
                append: false,
                nonblock: false,
            },
        )));

        fds[0] = read_fd as u32;
        fds[1] = write_fd as u32;

        info!("pipe: created rfd: {} wfd: {}", read_fd, write_fd);

        Ok(0)
    }

    pub fn sys_sync(&mut self) -> SysResult {
        //TODO: recursive sync
        use rcore_fs::vfs::FileSystem;
        VIRTUAL_FS.sync()?;
        Ok(0)
    }

    pub fn sys_mount(
        &mut self,
        source: *const u8,
        target: *const u8,
        fstype: *const u8,
        flags: usize,
        data: usize,
    ) -> SysResult {
        use crate::lkm::fs::*;
        use alloc::collections::btree_map::BTreeMap;
        let mut proc = self.process();
        let target = unsafe { check_and_clone_cstr(target)? };
        info!("mount: target: {}", target);
        let ret = match proc.cwd.path_resolve(&proc.cwd.cwd, &target, false)? {
            PathResolveResult::IsDir { dir } => {
                let source = unsafe { check_and_clone_cstr(source)? };
                let fstype = unsafe { check_and_clone_cstr(fstype)? };
                let fsm = FileSystemManager::get().read();
                drop(proc);
                let fs = fsm.mountFilesystem(self, &source, &fstype, flags as u64, data)?;
                dir.mount(fs)?;
                Ok(0 as usize)
            }
            PathResolveResult::NotExist { .. } => Err(SysError::ENOENT),
            PathResolveResult::IsFile { file, parent, .. } => Err(SysError::ENOTDIR),
        };
        info!("mount: {} success", target);
        ret
    }
    pub fn sys_sendfile(
        &mut self,
        out_fd: usize,
        in_fd: usize,
        offset_ptr: *mut usize,
        count: usize,
    ) -> SysResult {
        self.sys_copy_file_range(in_fd, offset_ptr, out_fd, 0 as *mut usize, count, 0)
    }

    pub fn sys_copy_file_range(
        &mut self,
        in_fd: usize,
        in_offset: *mut usize,
        out_fd: usize,
        out_offset: *mut usize,
        count: usize,
        flags: usize,
    ) -> SysResult {
        info!(
            "copy_file_range:BEG in: {}, out: {}, in_offset: {:?}, out_offset: {:?}, count: {} flags {}",
            in_fd, out_fd, in_offset, out_offset, count, flags
        );
        let proc = self.process();
        // We know it's save, pacify the borrow checker
        let proc_cell = UnsafeCell::new(proc);
        let in_file = unsafe { (*proc_cell.get()).get_file(in_fd)? };
        let out_file = unsafe { (*proc_cell.get()).get_file(out_fd)? };
        let mut buffer = [0u8; 1024];

        // for in_offset and out_offset
        // null means update file offset
        // non-null means update {in,out}_offset instead

        let mut read_offset = if !in_offset.is_null() {
            unsafe { *self.vm().check_read_ptr(in_offset)? }
        } else {
            in_file.seek(SeekFrom::Current(0))? as usize
        };

        let orig_out_file_offset = out_file.seek(SeekFrom::Current(0))?;
        let write_offset = if !out_offset.is_null() {
            out_file.seek(SeekFrom::Start(
                unsafe { *self.vm().check_read_ptr(out_offset)? } as u64,
            ))? as usize
        } else {
            0
        };

        // read from specified offset and write new offset back
        let mut bytes_read = 0;
        let mut total_written = 0;
        while bytes_read < count {
            let len = min(buffer.len(), count - bytes_read);
            let read_len = in_file.read_at(read_offset, &mut buffer[..len])?;
            if read_len == 0 {
                break;
            }
            bytes_read += read_len;
            read_offset += read_len;

            let mut bytes_written = 0;
            let mut rlen = read_len;
            while bytes_written < read_len {
                let write_len = out_file.write(&buffer[bytes_written..(bytes_written + rlen)])?;
                if write_len == 0 {
                    info!(
                        "copy_file_range:END_ERR in: {}, out: {}, in_offset: {:?}, out_offset: {:?}, count: {} = bytes_read {}, bytes_written {}, write_len {}",
                        in_fd, out_fd, in_offset, out_offset, count, bytes_read, bytes_written, write_len
                    );
                    return Err(SysError::EBADF);
                }
                bytes_written += write_len;
                rlen -= write_len;
            }
            total_written += bytes_written;
        }

        if !in_offset.is_null() {
            unsafe {
                in_offset.write(read_offset);
            }
        } else {
            in_file.seek(SeekFrom::Current(bytes_read as i64))?;
        }

        if !out_offset.is_null() {
            unsafe {
                out_offset.write(write_offset + total_written);
            }
            out_file.seek(SeekFrom::Start(orig_out_file_offset))?;
        }
        info!(
            "copy_file_range:END in: {}, out: {}, in_offset: {:?}, out_offset: {:?}, count: {} flags {}",
            in_fd, out_fd, in_offset, out_offset, count, flags
        );
        return Ok(total_written);
    }

    pub fn sys_fcntl(&mut self, fd: usize, cmd: usize, arg: usize) -> SysResult {
        info!("fcntl: fd: {}, cmd: {:x}, arg: {}", fd, cmd, arg);
        let mut proc = self.process();
        let file_like = proc.get_file_like(fd)?;
        file_like.fcntl(cmd, arg)
    }
}

impl Process {
    pub fn get_file_like(&mut self, fd: usize) -> Result<&mut FileLike, SysError> {
        self.files.get_mut(&fd).ok_or(SysError::EBADF)
    }
    pub fn get_file(&mut self, fd: usize) -> Result<&mut FileHandle, SysError> {
        match self.get_file_like(fd)? {
            FileLike::File(file) => Ok(file),
            _ => Err(SysError::EBADF),
        }
    }
    pub fn get_dir(&mut self, fd: usize) -> Result<Arc<INodeContainer>, SysError> {
        if fd == AT_FDCWD {
            Ok(Arc::clone(&self.cwd.cwd))
        } else {
            let file = self.get_file(fd)?;
            if file.metadata()?.type_ == FileType::Dir {
                Ok(Arc::clone(&file.inode_container))
            } else {
                Err(FsError::NotDir)?
            }
        }
    }
    pub fn get_file_const(&self, fd: usize) -> Result<&FileHandle, SysError> {
        match self.files.get(&fd).ok_or(SysError::EBADF)? {
            FileLike::File(file) => Ok(&file),
            _ => Err(SysError::EBADF),
        }
    }
    // TODO: Breaking change: breaks /proc/self/exe and /proc/self/fd
    //  They should be done in the right way.
}

impl From<FsError> for SysError {
    fn from(error: FsError) -> Self {
        match error {
            FsError::NotSupported => SysError::ENOSYS,
            FsError::NotFile => SysError::EISDIR,
            FsError::IsDir => SysError::EISDIR,
            FsError::NotDir => SysError::ENOTDIR,
            FsError::EntryNotFound => SysError::ENOENT,
            FsError::EntryExist => SysError::EEXIST,
            FsError::NotSameFs => SysError::EXDEV,
            FsError::InvalidParam => SysError::EINVAL,
            FsError::NoDeviceSpace => SysError::ENOMEM,
            FsError::DirRemoved => SysError::ENOENT,
            FsError::DirNotEmpty => SysError::ENOTEMPTY,
            FsError::WrongFs => SysError::EINVAL,
            FsError::DeviceError => SysError::EIO,
            FsError::SymLoop => SysError::ELOOP,
            FsError::NoDevice => SysError::ENXIO,
            FsError::IOCTLError => SysError::EINVAL,
            FsError::Again => SysError::EAGAIN,
            FsError::Busy => SysError::EBUSY,
        }
    }
}

bitflags! {
    struct AtFlags: usize {
        const EMPTY_PATH = 0x1000;
        const SYMLINK_NOFOLLOW = 0x100;
    }
}

bitflags! {
    struct OpenFlags: usize {
        /// read only
        const RDONLY = 0;
        /// write only
        const WRONLY = 1;
        /// read write
        const RDWR = 2;
        /// create file if it does not exist
        const CREATE = 1 << 6;
        /// error if CREATE and the file exists
        const EXCLUSIVE = 1 << 7;
        /// truncate file upon open
        const TRUNCATE = 1 << 9;
        /// append on each write
        const APPEND = 1 << 10;
    }
}

impl OpenFlags {
    fn readable(&self) -> bool {
        let b = self.bits() & 0b11;
        b == OpenFlags::RDONLY.bits() || b == OpenFlags::RDWR.bits()
    }
    fn writable(&self) -> bool {
        let b = self.bits() & 0b11;
        b == OpenFlags::WRONLY.bits() || b == OpenFlags::RDWR.bits()
    }
    fn to_options(&self) -> OpenOptions {
        OpenOptions {
            read: self.readable(),
            write: self.writable(),
            append: self.contains(OpenFlags::APPEND),
            nonblock: false,
        }
    }
}

#[derive(Debug)]
#[repr(packed)] // Don't use 'C'. Or its size will align up to 8 bytes.
pub struct LinuxDirent64 {
    /// Inode number
    ino: u64,
    /// Offset to next structure
    offset: u64,
    /// Size of this dirent
    reclen: u16,
    /// File type
    type_: u8,
    /// Filename (null-terminated)
    name: [u8; 0],
}

struct DirentBufWriter<'a> {
    buf: &'a mut [u8],
    ptr: *mut LinuxDirent64,
    rest_size: usize,
    written_size: usize,
}

impl<'a> DirentBufWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        DirentBufWriter {
            ptr: buf.as_mut_ptr() as *mut LinuxDirent64,
            rest_size: buf.len(),
            written_size: 0,
            buf,
        }
    }
    fn try_write(&mut self, inode: u64, type_: u8, name: &str) -> bool {
        let len = ::core::mem::size_of::<LinuxDirent64>() + name.len() + 1;
        let len = (len + 7) / 8 * 8; // align up
        if self.rest_size < len {
            return false;
        }
        let dent = LinuxDirent64 {
            ino: inode,
            offset: 0,
            reclen: len as u16,
            type_,
            name: [],
        };
        unsafe {
            self.ptr.write(dent);
            let name_ptr = self.ptr.add(1) as _;
            util::write_cstr(name_ptr, name);
            self.ptr = (self.ptr as *const u8).add(len) as _;
        }
        self.rest_size -= len;
        self.written_size += len;
        true
    }
}

bitflags! {
    pub struct DirentType: u8 {
        const DT_UNKNOWN  = 0;
        /// FIFO (named pipe)
        const DT_FIFO = 1;
        /// Character device
        const DT_CHR  = 2;
        /// Directory
        const DT_DIR  = 4;
        /// Block device
        const DT_BLK = 6;
        /// Regular file
        const DT_REG = 8;
        /// Symbolic link
        const DT_LNK = 10;
        /// UNIX domain socket
        const DT_SOCK  = 12;
        /// ???
        const DT_WHT = 14;
    }
}

impl DirentType {
    fn from_type(type_: &FileType) -> Self {
        match type_ {
            FileType::File => Self::DT_REG,
            FileType::Dir => Self::DT_DIR,
            FileType::SymLink => Self::DT_LNK,
            FileType::CharDevice => Self::DT_CHR,
            FileType::BlockDevice => Self::DT_BLK,
            FileType::Socket => Self::DT_SOCK,
            FileType::NamedPipe => Self::DT_FIFO,
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    dev: u64,
    /// inode number
    ino: u64,
    /// number of hard links
    nlink: u64,

    /// file type and mode
    mode: StatMode,
    /// user ID of owner
    uid: u32,
    /// group ID of owner
    gid: u32,
    /// padding
    _pad0: u32,
    /// device ID (if special file)
    rdev: u64,
    /// total size, in bytes
    size: u64,
    /// blocksize for filesystem I/O
    blksize: u64,
    /// number of 512B blocks allocated
    blocks: u64,

    /// last access time
    atime: Timespec,
    /// last modification time
    mtime: Timespec,
    /// last status change time
    ctime: Timespec,
}

#[cfg(target_arch = "mips")]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Timespec {
    pub sec: i32,
    pub nsec: i32,
}

#[cfg(target_arch = "mips")]
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    dev: u64,
    /// padding
    __pad1: u64,
    /// inode number
    ino: u64,
    /// file type and mode
    mode: StatMode,
    /// number of hard links
    nlink: u32,

    /// user ID of owner
    uid: u32,
    /// group ID of owner
    gid: u32,
    /// device ID (if special file)
    rdev: u64,
    /// padding
    __pad2: u64,
    /// total size, in bytes
    size: u64,

    /// last access time
    atime: Timespec,
    /// last modification time
    mtime: Timespec,
    /// last status change time
    ctime: Timespec,

    /// blocksize for filesystem I/O
    blksize: u32,
    /// padding
    __pad3: u32,
    /// number of 512B blocks allocated
    blocks: u64,
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "mips")))]
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// ID of device containing file
    dev: u64,
    /// inode number
    ino: u64,
    /// file type and mode
    mode: StatMode,
    /// number of hard links
    nlink: u32,

    /// user ID of owner
    uid: u32,
    /// group ID of owner
    gid: u32,
    /// device ID (if special file)
    rdev: u64,
    /// padding
    __pad: u64,
    /// total size, in bytes
    size: u64,
    /// blocksize for filesystem I/O
    blksize: u32,
    /// padding
    __pad2: u32,
    /// number of 512B blocks allocated
    blocks: u64,

    /// last access time
    atime: Timespec,
    /// last modification time
    mtime: Timespec,
    /// last status change time
    ctime: Timespec,
}

bitflags! {
    pub struct StatMode: u32 {
        const NULL  = 0;
        /// Type
        const TYPE_MASK = 0o170000;
        /// FIFO
        const FIFO  = 0o010000;
        /// character device
        const CHAR  = 0o020000;
        /// directory
        const DIR   = 0o040000;
        /// block device
        const BLOCK = 0o060000;
        /// ordinary regular file
        const FILE  = 0o100000;
        /// symbolic link
        const LINK  = 0o120000;
        /// socket
        const SOCKET = 0o140000;

        /// Set-user-ID on execution.
        const SET_UID = 0o4000;
        /// Set-group-ID on execution.
        const SET_GID = 0o2000;

        /// Read, write, execute/search by owner.
        const OWNER_MASK = 0o700;
        /// Read permission, owner.
        const OWNER_READ = 0o400;
        /// Write permission, owner.
        const OWNER_WRITE = 0o200;
        /// Execute/search permission, owner.
        const OWNER_EXEC = 0o100;

        /// Read, write, execute/search by group.
        const GROUP_MASK = 0o70;
        /// Read permission, group.
        const GROUP_READ = 0o40;
        /// Write permission, group.
        const GROUP_WRITE = 0o20;
        /// Execute/search permission, group.
        const GROUP_EXEC = 0o10;

        /// Read, write, execute/search by others.
        const OTHER_MASK = 0o7;
        /// Read permission, others.
        const OTHER_READ = 0o4;
        /// Write permission, others.
        const OTHER_WRITE = 0o2;
        /// Execute/search permission, others.
        const OTHER_EXEC = 0o1;
    }
}

impl StatMode {
    fn from_type_mode(type_: FileType, mode: u16) -> Self {
        let type_ = match type_ {
            FileType::File => StatMode::FILE,
            FileType::Dir => StatMode::DIR,
            FileType::SymLink => StatMode::LINK,
            FileType::CharDevice => StatMode::CHAR,
            FileType::BlockDevice => StatMode::BLOCK,
            FileType::Socket => StatMode::SOCKET,
            FileType::NamedPipe => StatMode::FIFO,
        };
        let mode = StatMode::from_bits_truncate(mode as u32);
        type_ | mode
    }
}

impl From<Metadata> for Stat {
    #[cfg(target_arch = "x86_64")]
    fn from(info: Metadata) -> Self {
        Stat {
            dev: info.dev as u64,
            ino: info.inode as u64,
            mode: StatMode::from_type_mode(info.type_, info.mode as u16),
            nlink: info.nlinks as u64,
            uid: info.uid as u32,
            gid: info.gid as u32,
            rdev: info.rdev as u64,
            size: info.size as u64,
            blksize: info.blk_size as u64,
            blocks: info.blocks as u64,
            atime: info.atime,
            mtime: info.mtime,
            ctime: info.ctime,
            _pad0: 0,
        }
    }

    #[cfg(target_arch = "mips")]
    fn from(info: Metadata) -> Self {
        Stat {
            dev: info.dev as u64,
            ino: info.inode as u64,
            mode: StatMode::from_type_mode(info.type_, info.mode as u16),
            nlink: info.nlinks as u32,
            uid: info.uid as u32,
            gid: info.gid as u32,
            rdev: 0,
            size: info.size as u64,
            blksize: info.blk_size as u32,
            blocks: info.blocks as u64,
            atime: Timespec {
                sec: info.atime.sec as i32,
                nsec: info.atime.nsec,
            },
            mtime: Timespec {
                sec: info.mtime.sec as i32,
                nsec: info.mtime.nsec,
            },
            ctime: Timespec {
                sec: info.ctime.sec as i32,
                nsec: info.ctime.nsec,
            },
            __pad1: 0,
            __pad2: 0,
            __pad3: 0,
        }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "mips")))]
    fn from(info: Metadata) -> Self {
        Stat {
            dev: info.dev as u64,
            ino: info.inode as u64,
            mode: StatMode::from_type_mode(info.type_, info.mode as u16),
            nlink: info.nlinks as u32,
            uid: info.uid as u32,
            gid: info.gid as u32,
            rdev: info.rdev as u64,
            size: info.size as u64,
            blksize: info.blk_size as u32,
            blocks: info.blocks as u64,
            atime: info.atime,
            mtime: info.mtime,
            ctime: info.ctime,
            __pad: 0,
            __pad2: 0,
        }
    }
}

const SEEK_SET: u8 = 0;
const SEEK_CUR: u8 = 1;
const SEEK_END: u8 = 2;

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct IoVec {
    /// Starting address
    base: *mut u8,
    /// Number of bytes to transfer
    len: usize,
}

/// A valid IoVecs request from user
#[derive(Debug)]
pub struct IoVecs(Vec<&'static mut [u8]>);

impl IoVecs {
    pub unsafe fn check_and_new(
        iov_ptr: *const IoVec,
        iov_count: usize,
        vm: &MemorySet,
        readv: bool,
    ) -> Result<Self, SysError> {
        let iovs = vm.check_read_array(iov_ptr, iov_count)?.to_vec();
        // check all bufs in iov
        for iov in iovs.iter() {
            // skip empty iov
            if iov.len == 0 {
                continue;
            }
            if readv {
                vm.check_write_array(iov.base, iov.len)?;
            } else {
                vm.check_read_array(iov.base, iov.len)?;
            }
        }
        let slices = iovs
            .iter()
            .map(|iov| slice::from_raw_parts_mut(iov.base, iov.len))
            .collect();
        Ok(IoVecs(slices))
    }

    pub fn read_all_to_vec(&self) -> Vec<u8> {
        let mut buf = self.new_buf(false);
        for slice in self.0.iter() {
            buf.extend(slice.iter());
        }
        buf
    }

    pub fn write_all_from_slice(&mut self, buf: &[u8]) {
        let mut copied_len = 0;
        for slice in self.0.iter_mut() {
            let copy_len = min(slice.len(), buf.len() - copied_len);
            if copy_len == 0 {
                continue;
            }

            slice[..copy_len].copy_from_slice(&buf[copied_len..copied_len + copy_len]);
            copied_len += copy_len;
        }
    }

    /// Create a new Vec buffer from IoVecs
    /// For readv:  `set_len` is true,  Vec.len = total_len.
    /// For writev: `set_len` is false, Vec.cap = total_len.
    pub fn new_buf(&self, set_len: bool) -> Vec<u8> {
        let total_len = self.0.iter().map(|slice| slice.len()).sum::<usize>();
        let mut buf = Vec::with_capacity(total_len);
        if set_len {
            unsafe {
                buf.set_len(total_len);
            }
        }
        buf
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct PollFd {
    fd: u32,
    events: PollEvents,
    revents: PollEvents,
}

bitflags! {
    pub struct PollEvents: u16 {
        /// There is data to read.
        const IN = 0x0001;
        /// Writing is now possible.
        const OUT = 0x0004;
        /// Error condition (return only)
        const ERR = 0x0008;
        /// Hang up (return only)
        const HUP = 0x0010;
        /// Invalid request: fd not open (return only)
        const INVAL = 0x0020;
    }
}

const FD_PER_ITEM: usize = 8 * size_of::<u32>();
const MAX_FDSET_SIZE: usize = 1024 / FD_PER_ITEM;

struct FdSet {
    bitset: &'static mut BitSlice<LittleEndian, u32>,
    origin: BitVec<LittleEndian, u32>,
}

impl FdSet {
    /// Initialize a `FdSet` from pointer and number of fds
    /// Check if the array is large enough
    fn new(vm: &MemorySet, addr: *mut u32, nfds: usize) -> Result<FdSet, SysError> {
        if addr.is_null() {
            Ok(FdSet {
                bitset: BitSlice::empty_mut(),
                origin: BitVec::new(),
            })
        } else {
            let len = (nfds + FD_PER_ITEM - 1) / FD_PER_ITEM;
            if len > MAX_FDSET_SIZE {
                return Err(SysError::EINVAL);
            }
            let slice = unsafe { vm.check_write_array(addr, len)? };
            let bitset: &'static mut BitSlice<LittleEndian, u32> = slice.into();
            debug!("bitset {:?}", bitset);

            // save the fdset, and clear it
            use alloc::prelude::ToOwned;
            let origin = bitset.to_owned();
            bitset.set_all(false);
            Ok(FdSet { bitset, origin })
        }
    }

    /// Try to set fd in `FdSet`
    /// Return true when `FdSet` is valid, and false when `FdSet` is bad (i.e. null pointer)
    /// Fd should be less than nfds
    fn set(&mut self, fd: usize) -> bool {
        if self.bitset.is_empty() {
            return false;
        }
        self.bitset.set(fd, true);
        true
    }

    /// Check to see whether `fd` is in original `FdSet`
    /// Fd should be less than nfds
    fn contains(&self, fd: usize) -> bool {
        if fd < self.bitset.len() {
            self.origin[fd]
        } else {
            false
        }
    }
}

pub const AT_FDCWD: usize = -100isize as usize;
