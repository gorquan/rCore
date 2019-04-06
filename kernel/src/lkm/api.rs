use super::*;

#[no_mangle]
pub extern "C" fn lkm_api_pong()-> usize{
    println!("Pong from Kernel Module!");
    println!("This indicates that a kernel module is successfully loaded into kernel and called a stub.");
    114514
}
