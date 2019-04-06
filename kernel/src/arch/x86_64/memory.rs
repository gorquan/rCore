use bit_allocator::BitAlloc;
use crate::consts::KERNEL_OFFSET;
// Depends on kernel
use crate::memory::{FRAME_ALLOCATOR, init_heap, active_table};
use super::{BootInfo, MemoryRegionType};
use rcore_memory::paging::*;
use once::*;
use log::*;

pub fn init(boot_info: &BootInfo) {
    assert_has_not_been_called!("memory::init must be called only once");
    init_frame_allocator(boot_info);
    init_device_vm_map();
    init_kernel_kseg2_map();
    init_heap();
    info!("memory: init end");
}

/// Init FrameAllocator and insert all 'Usable' regions from BootInfo.
fn init_frame_allocator(boot_info: &BootInfo) {
    let mut ba = FRAME_ALLOCATOR.lock();
    for region in boot_info.memory_map.iter() {
        if region.region_type == MemoryRegionType::Usable {
            ba.insert(region.range.start_frame_number as usize..region.range.end_frame_number as usize);
        }
    }
}

fn init_device_vm_map() {
    let mut page_table = active_table();
    // IOAPIC
    page_table.map(KERNEL_OFFSET + 0xfec00000, 0xfec00000).update();
    // LocalAPIC
    page_table.map(KERNEL_OFFSET + 0xfee00000, 0xfee00000).update();
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