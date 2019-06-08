extern crate rcore;
extern crate alloc;

pub mod ramfs;
#[no_mangle]
pub extern "C" fn init_module(){
    ramfs::RamFSBehav::registerRamFS();
}

