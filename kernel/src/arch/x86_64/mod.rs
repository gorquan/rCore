use bootloader::bootinfo::{BootInfo, MemoryRegionType};
use core::sync::atomic::*;
use log::*;

pub mod consts;
pub mod cpu;
pub mod driver;
pub mod gdt;
pub mod idt;
pub mod interrupt;
pub mod io;
pub mod memory;
pub mod paging;
pub mod rand;
pub mod ipi;
pub mod syscall;
pub mod timer;

static AP_CAN_INIT: AtomicBool = ATOMIC_BOOL_INIT;

/// The entry point of kernel
#[no_mangle] // don't mangle the name of this function
pub extern "C" fn _start(boot_info: &'static BootInfo) -> ! {
    let cpu_id = cpu::id();
    println!("Hello world! from CPU {}!", cpu_id);

    if cpu_id != 0 {
        while !AP_CAN_INIT.load(Ordering::Relaxed) {}
        other_start();
    }

    // First init log mod, so that we can print log info.
    //println!("Start logging");
    crate::logging::init();
    //println!("End logging");
    info!("{:#?}", boot_info);

    // Init trap handling.
    //println!("idt");
    idt::init();
    //println!("syscall");
    interrupt::fast_syscall::init();

    // Init physical memory management and heap.
    //println!("memory");
    //println!("memory");
    //println!("memory");
    memory::init(boot_info);

    // Now heap is available
    //println!("gdt");
    gdt::init();
    //println!("cpu");
    cpu::init();

    //println!("driver::init");
    driver::init();
    //println!("drivers::init");
    crate::drivers::init();

    //println!("fs");
    crate::rcore_fs::init();
    //panic!("Process!");
    //println!("process::init");

    crate::process::init();
    //println!("lkm::init");

    crate::lkm::manager::ModuleManager::init();

    crate::lkm::cdev::CDevManager::init();
    AP_CAN_INIT.store(true, Ordering::Relaxed);

    crate::kmain();
}

/// The entry point for other processors
fn other_start() -> ! {
    idt::init();
    gdt::init();
    cpu::init();
    interrupt::fast_syscall::init();
    crate::kmain();
}
