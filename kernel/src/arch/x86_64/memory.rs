use crate::consts::KERNEL_OFFSET;
use bit_allocator::BitAlloc;
// Depends on kernel
use super::{BootInfo, MemoryRegionType};
use crate::memory::{active_table, alloc_frame, init_heap, FRAME_ALLOCATOR};
use crate::HEAP_ALLOCATOR;
use alloc::vec::Vec;
use log::*;
use once::*;
use rcore_memory::paging::*;
use rcore_memory::PAGE_SIZE;

pub fn init(boot_info: &BootInfo) {
    //panic!("Crash here");
    println!("memory::init");
    assert_has_not_been_called!("memory::init must be called only once");
    println!("init_frame_allocator");
    init_frame_allocator(boot_info);
    //panic!("here");
    println!("init_device_vm_map");
    init_device_vm_map();
    println!("init_kernel_kseg2_map");
    init_kernel_kseg2_map();
    println!("init_heap");
    init_heap();
    println!("enlarge_heap");
    enlarge_heap();
    //panic!("end");
    info!("memory: init end");
}

/// Init FrameAllocator and insert all 'Usable' regions from BootInfo.
fn init_frame_allocator(boot_info: &BootInfo) {
    let mut ba = FRAME_ALLOCATOR.lock();
    for region in boot_info.memory_map.iter() {
        if region.region_type == MemoryRegionType::Usable {
            ba.insert(
                region.range.start_frame_number as usize..region.range.end_frame_number as usize,
            );
        }
    }
}

fn init_device_vm_map() {
    let mut page_table = active_table();
    // IOAPIC
    page_table
        .map(KERNEL_OFFSET + 0xfec00000, 0xfec00000)
        .update();
    // LocalAPIC
    page_table
        .map(KERNEL_OFFSET + 0xfee00000, 0xfee00000)
        .update();
}

fn enlarge_heap() {
    let mut page_table = active_table();
    let mut addrs = Vec::new();
    let va_offset = KERNEL_OFFSET + 0xe0000000;
    for i in 0..16384 {
        let page = alloc_frame().unwrap();
        let va = KERNEL_OFFSET + 0xe0000000 + page;
        if let Some((ref mut addr, ref mut len)) = addrs.last_mut() {
            if *addr - PAGE_SIZE == va {
                *len += PAGE_SIZE;
                *addr -= PAGE_SIZE;
                continue;
            }
        }
        addrs.push((va, PAGE_SIZE));
    }
    for (addr, len) in addrs.into_iter() {
        for va in (addr..(addr + len)).step_by(PAGE_SIZE) {
            page_table.map(va, va - va_offset).update();
        }
        info!("Adding {:#X} {:#X} to heap", addr, len);
        unsafe {
            HEAP_ALLOCATOR.lock().init(addr, len);
        }
    }
}

fn init_kernel_kseg2_map(){
    let mut page_table=active_table();
    // Dirty hack here:
    // We do not really need the mapping. Indeed, we only need the second-level page table.
    // Second-level page table item can then be copied to all page tables safely.
    // This hack requires the page table not to recycle the second level page table on unmap.
    println!("Page table[510] before mapped: {}", unsafe{*(0xffff_ffff_ffff_fff0 as *const usize)});
    println!("Page table[175] before mapped: {}", unsafe{*(0xffff_ffff_ffff_f578 as *const usize)});
    println!("Page table[509] before mapped: {}", unsafe{*(0xffff_ffff_ffff_ffe8 as *const usize)});
    page_table.map(0xfffffe8000000000, 0x0).update();
    page_table.unmap(0xfffffe8000000000);
    println!("Page table[509] after mapped: {}", unsafe{*(0xffff_ffff_ffff_ffe8 as *const usize)});

}