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
/*
use super::cdev::{CharDev, FileOperations};
use alloc::boxed::Box;
use alloc::sync::Arc;
use crate::rcore_fs::vfs::Result;
use alloc::string::String;
use alloc::str;
use alloc::vec::Vec;

struct Internal{
    foo: usize,
    sentence: Vec<u8>

}
fn toInternal(s: usize)->&'static mut Internal{
    unsafe{&mut(*(s as *mut Internal)) as &'static mut Internal}
}
fn hello_open()->usize{
    let internal=Box::new(Internal{
        foo: 0,
        sentence: String::from("The essence of human is repeater.\n").into_bytes()
    });
    unsafe {Box::into_raw(internal) as usize}
}
fn hello_close(s: usize){
    unsafe {Box::from_raw(s as *mut Internal);}// will be dropped
}

fn hello_read(file: usize, buf: &mut [u8]) -> Result<usize>{
    let internal=toInternal(file);
    for pos in buf.iter_mut(){
        *pos=internal.sentence[internal.foo];
        internal.foo+=1;
        if internal.foo==internal.sentence.len(){
            internal.foo=0;
        }
    }
    Ok(buf.len())
}
pub fn get_cdev()->CharDev{
    CharDev{
        parent_module: None,
        file_op: Arc::new(FileOperations{
            open: Some(hello_open),
            read: Some(hello_read),
            read_at: None,
            write: None,
            write_at: None,
            seek: None,
            set_len: None,
            sync_all: None,
            sync_data: None,
            metadata: None,
            read_entry: None,
            poll: None,
            io_control: None,
            close: Some(hello_close)
        })
    }
}
*/
