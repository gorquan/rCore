// Simple kernel memory set for kernel virtual memory
use crate::arch::ipi::*;
use crate::arch::paging::ActivePageTable;
use crate::consts::*;
use crate::memory::{active_table, GlobalFrameAlloc};
use crate::sync::SpinLock as Mutex;
use alloc::vec::*;
use buddy_system_allocator::*;
use core::alloc::Layout;
use core::ptr::NonNull;
use lazy_static::lazy_static;
use rcore_memory::memory_set::handler::{ByFrame, MemoryHandler};
use rcore_memory::memory_set::MemoryAttr;
use rcore_memory::{Page, PAGE_SIZE};
//Allocated virtual memory space by pages. returns some vaddr.
pub trait MemorySpaceManager {
    fn new() -> Self;
    fn alloc(&mut self, size: usize) -> Option<(usize, usize)>;
    fn free(&mut self, target: (usize, usize));
    fn active_table(&self) -> ActivePageTable;
}

//The most simple strategy: no free and allocate ahead.
pub struct LinearManager {
    last_page: usize,
}
pub const KSEG2_START: usize = 0xffff_fe80_0000_0000;

impl MemorySpaceManager for LinearManager {
    fn new() -> LinearManager {
        LinearManager { last_page: 0 }
    }
    fn alloc(&mut self, size: usize) -> Option<(usize, usize)> {
        let mut required_pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;

        let current = self.last_page * PAGE_SIZE + KSEG2_START;
        self.last_page += required_pages;
        Some((current, required_pages * PAGE_SIZE))
    }

    fn free(&mut self, (addr, size): (usize, usize)) {
        //Do nothing.
    }
    fn active_table(&self) -> ActivePageTable {
        active_table()
    }
}

// 512 GiB is a large space, and we don't need to worry about internal fragmentation.
// What kind of kernel program will try to allocate 256 GiB memory?
// 27 layers is enough, since the minimal unit is a block.

pub struct BuddyManager(pub Heap);

impl MemorySpaceManager for BuddyManager {
    fn new() -> Self {
        let mut vmm = BuddyManager(Heap::empty());
        unsafe {
            vmm.0.init(KSEG2_START, 0x8000000000);
            //vmm.0.add_to_heap(KSEG2_START, KSEG2_START+0x8000000000);
        }
        vmm
    }

    fn alloc(&mut self, size: usize) -> Option<(usize, usize)> {
        let mut required_pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
        let ret = self
            .0
            .alloc(Layout::from_size_align(required_pages * PAGE_SIZE, 1).ok()?);
        match ret {
            Ok(start) => Some((start.as_ptr() as usize, required_pages * PAGE_SIZE)),
            Err(err) => {
                error!("[KVMM] allocation failed!");
                None
            }
        }
    }

    fn free(&mut self, target: (usize, usize)) {
        self.0.dealloc(
            unsafe { NonNull::new_unchecked(target.0 as *mut u8) },
            Layout::from_size_align(target.1, 1).unwrap(),
        )
    }

    fn active_table(&self) -> ActivePageTable {
        active_table()
    }
}
/*
// The by-frame manager for kseg2 memory. Does no check on its allocation and freeing.
pub struct VKMemManager<T: MemorySpaceManager>{
    allocator: ByFrame<GlobalFrameAlloc>,
    space_man: T
}

impl<T: MemorySpaceManager> VKMemManager<T>{
    fn new()->VKMemManager<T>{

        VKMemManager{
            allocator: ByFrame::new(GlobalFrameAlloc),
            space_man: T::new()
        }
    }
    fn getSpace(&mut self)->&mut T {
        return &mut (self.space_man);
    }

    fn map_pages(&mut self, start_addr: usize, end_addr: usize, attr: &MemoryAttr){
        let mut active_pt=active_table();
        for p in Page::range_of(start_addr, end_addr) {
            self.allocator.map(&mut active_pt, p.start_address(),attr);
        }
    }
    fn unmap_pages(&mut self, start_addr: usize, end_addr: usize){
        let mut active_pt=active_table();
        for p in Page::range_of(start_addr,end_addr){
            self.allocator.unmap(&mut active_pt,p.start_address());
        }
        //Some IPI trick here, but we don't care now.

    }
    fn alloc(&mut self, size: usize, attr: &MemoryAttr)->usize{
        let mut required_pages=size/PAGE_SIZE;
        if size%PAGE_SIZE>0{
            required_pages=required_pages+1;
        }
        let (start_addr, area_size)=self.space_man.alloc(size).unwrap();
        self.map_pages(start_addr, start_addr+required_pages*PAGE_SIZE, attr);
        start_addr
    }
    fn free(&mut self, addr: usize, size: usize){
        let mut required_pages=size/PAGE_SIZE;
        if size%PAGE_SIZE>0{
            required_pages=required_pages+1;
        }
        self.unmap_pages(addr, addr+required_pages*PAGE_SIZE);
        self.space_man.free((addr, size));
    }
}
*/
type VirtualMemorySpaceManager = LinearManager;
type LockedVMM = Mutex<VirtualMemorySpaceManager>;
lazy_static! {
    pub static ref KERNELVM_MANAGER: LockedVMM = Mutex::new(VirtualMemorySpaceManager::new());
}

// Represents a contiguous virtual area: like the ancient loader.
// Use RAII for exception handling
pub struct VirtualSpace {
    start: usize,
    size: usize,
    areas: Vec<VirtualArea>,
    allocator: &'static LockedVMM,
    page_allocator: ByFrame<GlobalFrameAlloc>,
}

impl VirtualSpace {
    pub fn new(allocator: &'static LockedVMM, size: usize) -> Option<VirtualSpace> {
        let mut vmm = allocator.lock();
        let (start, rsize) = vmm.alloc(size)?;
        Some(VirtualSpace {
            start: start,
            size: rsize,
            areas: Vec::new(),
            allocator: allocator,
            page_allocator: ByFrame::new(GlobalFrameAlloc),
        })
    }
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn size(&self) -> usize {
        self.size
    }
    fn map_pages(&mut self, start_addr: usize, end_addr: usize, attr: &MemoryAttr) {
        let mut active_pt = active_table();
        for p in Page::range_of(start_addr, end_addr) {
            self.page_allocator
                .map(&mut active_pt, p.start_address(), attr);
        }
    }
    fn unmap_pages(&mut self, start_addr: usize, end_addr: usize) {
        let mut active_pt = active_table();
        for p in Page::range_of(start_addr, end_addr) {
            self.page_allocator.unmap(&mut active_pt, p.start_address());
        }
        //Some IPI trick here, but we don't care now.
    }
    pub fn add_area(
        &mut self,
        start_addr: usize,
        end_addr: usize,
        attr: &MemoryAttr,
    ) -> &VirtualArea {
        let area = VirtualArea::new(start_addr, end_addr - start_addr, attr, self);
        self.areas.push(area);
        self.areas.last().unwrap()
    }
}

impl Drop for VirtualSpace {
    fn drop(&mut self) {
        for mut v in self.areas.iter_mut() {
            v.unmap(self.allocator, &mut self.page_allocator);
        }
    }
}

pub struct VirtualArea {
    start: usize,
    end: usize,
    attr: MemoryAttr,
}
impl VirtualArea {
    pub fn new(
        page_addr: usize,
        size: usize,
        attr: &MemoryAttr,
        parent: &mut VirtualSpace,
    ) -> VirtualArea {
        let aligned_start_addr = page_addr - page_addr % PAGE_SIZE;
        let mut aligned_end = (page_addr + size + PAGE_SIZE - 1);
        aligned_end = aligned_end - aligned_end % PAGE_SIZE;
        let mut active_pt = parent.allocator.lock().active_table();
        for p in Page::range_of(aligned_start_addr, aligned_end) {
            parent
                .page_allocator
                .map(&mut active_pt, p.start_address(), attr);
        }
        debug!("[VMM] Allocating");
        //invoke_on_allcpu(tlb_shootdown, (aligned_start_addr, aligned_end),true);
        debug!("[VMM] Allocated!");
        VirtualArea {
            start: aligned_start_addr,
            end: aligned_end,
            attr: attr.clone(),
        }
    }
    pub fn unmap(&mut self, allocator: &LockedVMM, parent: &mut ByFrame<GlobalFrameAlloc>) {
        let mut active_pt = allocator.lock().active_table();
        for p in Page::range_of(self.start, self.end) {
            parent.unmap(&mut active_pt, p.start_address());
        }
        invoke_on_allcpu(tlb_shootdown, (self.start, self.end), true);
    }
}
