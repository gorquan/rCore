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
use rcore_fs_mountfs::MNode as INodeContainer;
use spin::RwLock;

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
        let root = super::VIRTUAL_FS.root_inode();
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
