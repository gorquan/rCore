use crate::drivers::BlockDriver;
use crate::fs::device;
use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::any::Any;
use core::mem::uninitialized;
use core::str;
use rcore_fs::dev::block_cache::BlockCache;
use rcore_fs::vfs::*;
use rcore_fs_sfs::{INodeId, SimpleFileSystem};
use spin::RwLock;

/// The filesystem on which all the other filesystems are mounted
pub struct VirtualFS {
    filesystem: Arc<FileSystem>,
    mountpoints: BTreeMap<INodeId, Arc<RwLock<VirtualFS>>>,
    self_mountpoint: Option<Arc<INodeContainer>>,
    self_ref: Weak<RwLock<VirtualFS>>,
}

#[derive(Clone)]
pub struct INodeContainer {
    pub inode: Arc<INode>,
    pub vfs: Arc<RwLock<VirtualFS>>,
    self_ref: Weak<INodeContainer>,
}

impl VirtualFS {
    pub fn init() -> Arc<RwLock<Self>> {
        VirtualFS {
            filesystem: VirtualFS::init_mount_sfs(),
            mountpoints: BTreeMap::new(),
            self_mountpoint: None,
            self_ref: Weak::default(),
        }
        .wrap()
    }

    /// Wrap pure `VirtualFS` with `Arc<RwLock<..>>`.
    /// Used in constructors.
    fn wrap(self) -> Arc<RwLock<Self>> {
        // Create an Arc, make a Weak from it, then put it into the struct.
        // It's a little tricky.
        let fs = Arc::new(RwLock::new(self));
        let weak = Arc::downgrade(&fs);
        let ptr = Arc::into_raw(fs) as *mut RwLock<Self>;
        unsafe {
            (*ptr).write().self_ref = weak;
            Arc::from_raw(ptr)
        }
    }

    // TODO: mount sfs onto root.
    // This is somehow hard work to do: since you may want to unify the process.
    // 1. Boot from a filesystem like initramfs, which can be a readonly SFS mounted onto root.
    //    This means you can bundle kernel modules into kernel by packaging them in initramfs.
    // 2. Mount /dev and place /dev/sda (while naming /dev/sda itself is a hard problem that is related with universal device management).
    // 3. Remount root, replacing initramfs with /dev/sda (this requires connecting filesystem to device system).
    //    A hacky approach to avoid implementing re-mounting is to mount /dev/sda under initramfs and perform a chroot.
    //    But in this way you must simulate chroot-jailbreaking behaviour properly: even if some application breaks the jail, it should not ever touch initramfs, or you're caught cheating.
    //    Or... you can swap the SFS with VIRTUAL_FS?

    fn init_mount_sfs() -> Arc<FileSystem> {
        let device = {
            #[cfg(not(feature = "link_user"))]
            let device = {
                #[cfg(any(
                    target_arch = "riscv32",
                    target_arch = "riscv64",
                    target_arch = "x86_64"
                ))]
                {
                    let driver = BlockDriver(
                        crate::drivers::BLK_DRIVERS
                            .read()
                            .iter()
                            .next()
                            .expect("Block device not found")
                            .clone(),
                    );
                    // enable block cache
                    Arc::new(BlockCache::new(driver, 0x100))
                    // Arc::new(driver)
                }
                #[cfg(target_arch = "aarch64")]
                {
                    unimplemented!()
                }
            };
            #[cfg(feature = "link_user")]
            let device = {
                extern "C" {
                    fn _user_img_start();
                    fn _user_img_end();
                }
                info!(
                    "SFS linked to kernel, from {:08x} to {:08x}",
                    _user_img_start as usize, _user_img_end as usize
                );
                Arc::new(unsafe { device::MemBuf::new(_user_img_start, _user_img_end) })
            };

            let sfs = SimpleFileSystem::open(device).expect("failed to open SFS");
            sfs
        };
        device
    }

    pub fn root_inode(&self) -> Arc<INodeContainer> {
        INodeContainer {
            inode: self.filesystem.root_inode(),
            vfs: self.self_ref.upgrade().unwrap(),
            self_ref: Weak::default(),
        }
        .wrap()
    }
}

#[derive(Clone)]
pub struct PathConfig {
    pub root: Arc<INodeContainer>, // ensured to be a dir.
    pub cwd: Arc<INodeContainer>,  // ensured to be a dir.
}

/// The enum used to represent result of a successful path resolve.
pub enum PathResolveResult {
    IsDir {
        // You can always get the parent directory by inode, so no necessity to take with parent.
        dir: Arc<INodeContainer>,
    },
    IsFile {
        // If it is a file, its parent must have been touched.
        // This is also returned for further symbol resolving, since resolving symbol needs parent directory.
        file: Arc<INodeContainer>,
        parent: Arc<INodeContainer>,
        name: String,
    },
    NotExist {
        // If it is not found, its parent must have been touched. This is useful when dealing with syscalls like creat or rename.
        parent: Arc<INodeContainer>,
        name: String,
    },
}

// Path resolution must be done with a root.
// A better name is "Filesystem Selector", like the "segment selector".
impl PathConfig {
    pub fn init_root() -> PathConfig {
        let root = super::get_virtual_fs().read().root_inode();
        let cwd = root.clone();
        PathConfig { root, cwd }
    }

    pub fn path_resolve(
        &self,
        cwd: &Arc<INodeContainer>,
        path: &str,
        resolve_last_symbol: bool,
    ) -> Result<PathResolveResult> {
        let mut follow_counter = 40;
        let depth_counter = 10;
        let r = self.resolve_path(cwd, path, &mut follow_counter, depth_counter)?;
        if resolve_last_symbol {
            if let PathResolveResult::IsFile { file, parent, .. } = r {
                return self.resolve_symbol_recursively(
                    &parent,
                    &file,
                    &mut follow_counter,
                    depth_counter,
                );
            }
        }
        Ok(r)
    }

    pub fn resolve_parent(&self, cwd: &Arc<INodeContainer>) -> Arc<INodeContainer> {
        cwd.find(self.has_reached_root(&cwd), "..").unwrap() // There is no reason that this can fail, as long as cwd is really a directory.
    }

    /// This call is used by getcwd() to detect possible leaks.
    /// All files are organized in a big tree, so it will eventually achieve the root.
    pub unsafe fn force_resolve_parent(&self, cwd: &Arc<INodeContainer>) -> Arc<INodeContainer> {
        cwd.find(false, "..").unwrap()
    }

    pub fn resolve_path(
        &self,
        cwd: &Arc<INodeContainer>,
        path: &str,
        follow_counter: &mut usize,
        depth_counter: usize,
    ) -> Result<PathResolveResult> {
        debug!("Path resolution {}", path);
        let mut cwd = Arc::clone({
            if path.starts_with("/") {
                &self.root
            } else {
                if cwd.inode.metadata().unwrap().type_ != FileType::Dir {
                    return Err(FsError::NotDir);
                }
                cwd //must be a dir, or an error will be thrown.
            }
        });
        let parts: Vec<&str> = path.split("/").collect();
        let (last_part, mid_part) = parts.split_last().unwrap();

        for part in mid_part.iter() {
            if *part == "" {
                continue;
            }
            debug!("Resolve part: {}", part);
            let next = cwd.find(self.has_reached_root(&cwd), part)?;
            debug!("solve link");
            // Try solve symbolic link.
            let symlink_solve_result =
                self.resolve_symbol_recursively(&cwd, &next, follow_counter, depth_counter)?;
            match symlink_solve_result {
                PathResolveResult::IsDir { dir } => {
                    cwd = dir;
                }
                PathResolveResult::IsFile { .. } => {
                    return Err(FsError::NotDir);
                }
                PathResolveResult::NotExist { .. } => {
                    return Err(FsError::EntryNotFound);
                }
            }
        }
        debug!("Last part {}", last_part);
        // Resolving last part.
        let next = cwd.find(self.has_reached_root(&cwd), last_part);
        debug!("match next");
        match next {
            Ok(next) => {
                debug!("Ok!");
                //No extra check needed, since extra work can be done through check.
                if next.inode.metadata().unwrap().type_ == FileType::Dir {
                    Ok(PathResolveResult::IsDir { dir: next })
                } else {
                    Ok(PathResolveResult::IsFile {
                        parent: cwd,
                        file: next,
                        name: String::from(*last_part),
                    })
                }
            }
            Err(FsError::EntryNotFound) => Ok(PathResolveResult::NotExist {
                name: String::from(*last_part),
                parent: cwd,
            }),
            Err(x) => Err(x),
        }
    }

    /// Resolves symbol by one layer.
    ///
    /// TODO:
    ///   Linux proc fs has some anti-POSIX magics here, like /proc/[pid]/root.
    ///   In those cases, those magics points to strange places, without following symlink rules.
    ///   This hack can be achieved here.
    pub fn resolve_symbol(
        &self,
        cwd: &Arc<INodeContainer>,
        symbol: &Arc<INodeContainer>,
        follow_counter: &mut usize,
        depth_counter: usize,
    ) -> Result<PathResolveResult> {
        if depth_counter == 0 {
            return Err(FsError::SymLoop);
        }
        if *follow_counter > 0 {
            *follow_counter -= 1;
            let mut content = [0u8; 256];
            let len = symbol.inode.read_at(0, &mut content)?;
            if let Ok(path) = str::from_utf8(&content[..len]) {
                self.resolve_path(cwd, path, follow_counter, depth_counter - 1)
            } else {
                return Err(FsError::NotDir);
            }
        } else {
            Err(FsError::SymLoop)
        }
    }
    /// Resolves symbol recursively.
    /// Note that a not-found will cause the resolved symbol pointing to the final file.
    pub fn resolve_symbol_recursively(
        &self,
        cwd: &Arc<INodeContainer>,
        symbol: &Arc<INodeContainer>,
        follow_counter: &mut usize,
        depth_counter: usize,
    ) -> Result<PathResolveResult> {
        let mut current_symbol_dir = Arc::clone(cwd);
        let mut current_symbol = Arc::clone(symbol);
        let mut current_name = String::new();
        while current_symbol.inode.metadata().unwrap().type_ == FileType::SymLink {
            let resolve_result = self.resolve_symbol(
                &current_symbol_dir,
                &current_symbol,
                follow_counter,
                depth_counter,
            )?;
            match resolve_result {
                PathResolveResult::NotExist { .. } => {
                    return Ok(resolve_result);
                }
                PathResolveResult::IsDir { .. } => {
                    return Ok(resolve_result);
                }
                PathResolveResult::IsFile { file, parent, name } => {
                    current_symbol = file;
                    current_symbol_dir = parent;
                    current_name = name;
                }
            }
        }
        if current_symbol.inode.metadata().unwrap().type_ == FileType::Dir {
            Ok(PathResolveResult::IsDir {
                dir: current_symbol,
            })
        } else {
            Ok(PathResolveResult::IsFile {
                file: current_symbol,
                parent: current_symbol_dir,
                name: current_name,
            })
        }
    }
    pub fn has_reached_root(&self, current: &INodeContainer) -> bool {
        Arc::ptr_eq(&current.vfs, &self.root.vfs)
            && self.root.inode.metadata().unwrap().inode == current.inode.metadata().unwrap().inode
    }
}

// XXX: what's the meaning?
// The unsafe filesystem for Stdin, Stdout, anonymous pipe and so on.
// If you don't touch it you will not break it.
// But in fact you should detect file operations (e.g. fstat) on virtual files and prevent them.
pub static mut ANONYMOUS_FS: Option<Arc<RwLock<VirtualFS>>> = None;

pub fn get_anonymous_fs() -> &'static Arc<RwLock<VirtualFS>> {
    unsafe { ANONYMOUS_FS.as_ref().unwrap() }
}

impl INodeContainer {
    /// Wrap pure `INode` with `Arc<..>`.
    /// Used in constructors.
    fn wrap(self) -> Arc<Self> {
        // Create an Arc, make a Weak from it, then put it into the struct.
        // It's a little tricky.
        let inode = Arc::new(self);
        let weak = Arc::downgrade(&inode);
        let ptr = Arc::into_raw(inode) as *mut Self;
        unsafe {
            (*ptr).self_ref = weak;
            Arc::from_raw(ptr)
        }
    }

    /// Mount file system `fs` at this INode
    pub fn mount(self: &Arc<Self>, fs: Arc<FileSystem>) -> Result<Arc<RwLock<VirtualFS>>> {
        let new_fs = VirtualFS {
            filesystem: fs,
            mountpoints: BTreeMap::new(),
            self_mountpoint: Some(Arc::clone(self)),
            self_ref: Weak::default(),
        }
        .wrap();
        let inode_id = self.inode.metadata()?.inode;
        let mut self_fs = self.vfs.write();
        self_fs.mountpoints.insert(inode_id, new_fs.clone());
        Ok(new_fs)
    }

    /// Get the root INode of the mounted fs at here.
    /// Return self if no mounted fs.
    fn overlaid_mount_point(&self) -> Arc<INodeContainer> {
        let inode_id = self.metadata().unwrap().inode;
        if let Some(sub_vfs) = self.vfs.read().mountpoints.get(&inode_id) {
            sub_vfs.read().root_inode()
        } else {
            self.self_ref.upgrade().unwrap()
        }
    }

    pub fn is_very_root(&self) -> bool {
        PathConfig::init_root().has_reached_root(self)
    }
    pub fn is_root_inode(&self) -> bool {
        self.inode.fs().root_inode().metadata().unwrap().inode
            == self.inode.metadata().unwrap().inode
    }

    /// Creates an anonymous inode.
    /// Should not be used as a location at any time, or be totally released at any time.
    pub unsafe fn anonymous_inode(inode: Arc<INode>) -> Arc<INodeContainer> {
        INodeContainer {
            inode,
            vfs: Arc::clone(get_anonymous_fs()),
            self_ref: Weak::default(),
        }
        .wrap()
    }
    pub unsafe fn is_anonymous(&self) -> bool {
        Arc::ptr_eq(&self.vfs, get_anonymous_fs())
    }

    pub fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<Self>> {
        Ok(INodeContainer {
            inode: self.inode.create(name, type_, mode)?,
            vfs: self.vfs.clone(),
            self_ref: Weak::default(),
        }
        .wrap())
    }

    /// Does a one-level finding.
    pub fn find(&self, root: bool, name: &str) -> Result<Arc<Self>> {
        match name {
            "" | "." => Ok(self.self_ref.upgrade().unwrap()),
            ".." => {
                // Going Up
                // We need to check these things:
                // 1. Is going forward allowed, considering the current root?
                // 2. Is going forward trespassing the filesystem border,
                //    thus requires falling back to parent of original_mountpoint?
                // TODO: check going up.
                if root {
                    Ok(self.self_ref.upgrade().unwrap())
                } else if self.is_root_inode() {
                    // Here is mountpoint.
                    match &self.vfs.read().self_mountpoint {
                        Some(inode) => inode.find(root, ".."),
                        // root fs
                        None => Ok(self.self_ref.upgrade().unwrap()),
                    }
                } else {
                    // Not trespassing filesystem border. Parent and myself in the same filesystem.
                    Ok(INodeContainer {
                        inode: self.inode.find(name)?, // Going up is handled by the filesystem. A better API?
                        vfs: self.vfs.clone(),
                        self_ref: Weak::default(),
                    }
                    .wrap())
                }
            }
            _ => {
                // Going down may trespass the filesystem border.
                // An INode replacement is required here.
                Ok(INodeContainer {
                    inode: self.inode.find(name)?,
                    vfs: self.vfs.clone(),
                    self_ref: Weak::default(),
                }
                .wrap()
                .overlaid_mount_point())
            }
        }
    }

    /// If `child` is a child of `self`, return its name.
    pub fn find_name_by_child(
        self: &Arc<INodeContainer>,
        child: &Arc<INodeContainer>,
    ) -> Result<String> {
        for index in 0.. {
            let name = self.inode.get_entry(index)?;
            match name.as_ref() {
                "." | ".." => {}
                _ => {
                    let queryback = self.find(false, &name)?.overlaid_mount_point();
                    // TODO: mountpoint check!
                    debug!("checking name {}", name);
                    if Arc::ptr_eq(&queryback.vfs, &child.vfs)
                        && queryback.inode.metadata()?.inode == child.inode.metadata()?.inode
                    {
                        return Ok(name);
                    }
                }
            }
        }
        Err(FsError::EntryNotFound)
    }
}

impl FileSystem for VirtualFS {
    fn sync(&self) -> Result<()> {
        self.filesystem.sync()?;
        Ok(())
    }

    fn root_inode(&self) -> Arc<INode> {
        self.root_inode()
    }

    fn info(&self) -> FsInfo {
        self.filesystem.info()
    }
}

impl INode for INodeContainer {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        self.inode.read_at(offset, buf)
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        self.inode.write_at(offset, buf)
    }

    fn poll(&self) -> Result<PollStatus> {
        self.inode.poll()
    }

    fn metadata(&self) -> Result<Metadata> {
        self.inode.metadata()
    }

    fn set_metadata(&self, metadata: &Metadata) -> Result<()> {
        self.inode.set_metadata(metadata)
    }

    fn sync_all(&self) -> Result<()> {
        self.inode.sync_all()
    }

    fn sync_data(&self) -> Result<()> {
        self.inode.sync_data()
    }

    fn resize(&self, len: usize) -> Result<()> {
        self.inode.resize(len)
    }

    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>> {
        Ok(self.create(name, type_, mode)?)
    }

    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()> {
        let other = &other
            .downcast_ref::<Self>()
            .ok_or(FsError::NotSameFs)?
            .inode;
        self.inode.link(name, other)
    }

    fn unlink(&self, name: &str) -> Result<()> {
        self.inode.unlink(name)
    }

    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()> {
        let target = &target
            .downcast_ref::<Self>()
            .ok_or(FsError::NotSameFs)?
            .inode;
        self.inode.move_(old_name, target, new_name)
    }

    fn find(&self, name: &str) -> Result<Arc<INode>> {
        Ok(self.find(false, name)?)
    }

    fn get_entry(&self, id: usize) -> Result<String> {
        self.inode.get_entry(id)
    }

    fn io_control(&self, cmd: u32, data: usize) -> Result<()> {
        self.inode.io_control(cmd, data)
    }

    fn fs(&self) -> Arc<FileSystem> {
        self.inode.fs()
    }

    fn as_any_ref(&self) -> &Any {
        self
    }
}
