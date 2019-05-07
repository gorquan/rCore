use alloc::boxed::Box;
use core::borrow::Borrow;

struct Node {
    start: usize,
    size: usize,
    next: Option<Box<Node>>,
}

type Slot = *mut Option<Box<Node>>;
type PtrNode = *mut Box<Node>;
unsafe fn next_addr(slot: Slot) -> Option<PtrNode> {
    if (*slot).is_none() {
        None
    } else {
        Some((*slot).as_mut().unwrap() as PtrNode)
    }
}

pub struct FreeList {
    node: Option<Box<Node>>,
}

impl FreeList {
    pub fn new(start: usize, size: usize) -> FreeList {
        let root = Box::new(Node {
            start: start,
            size: size,
            next: None,
        });
        FreeList { node: Some(root) }
    }
    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        unsafe {
            let mut iterator = &mut (self.node) as Slot;
            let mut pos: Option<PtrNode> = next_addr(iterator);
            while pos.is_some() {
                let mut ptr = pos.clone().unwrap();
                if (*ptr).size >= size {
                    if (*ptr).size > size {
                        (*ptr).size -= size;
                        let ret = (*ptr).start;
                        (*ptr).start += size;
                        return Some(ret);
                    } else {
                        let ret = (*ptr).start;
                        let mut placeholder = None;
                        ::core::mem::swap(&mut placeholder, &mut ((*ptr).next));
                        ::core::mem::swap(
                            &mut placeholder,
                            &mut (*iterator) as &mut Option<Box<Node>>,
                        );
                        drop(placeholder); //This is unnecessary, but we do this.
                        return Some(ret);
                    }
                } else {
                    iterator = &mut ((*pos.unwrap()).next) as Slot;
                    pos = next_addr(iterator);
                }
            }
            None
        }
    }
    /*
    pub fn free(&mut self, start: usize, size: usize){
        unsafe{
            let mut iterator = &mut(self.node) as Slot;
            let mut pos: Option<PtrNode>=next_addr(iterator);
            while pos.is_some(){
                let mut ptr=pos.clone().unwrap();
                if (*ptr).start>start{
                    if start+size==(*ptr).start{
                        (*ptr).start=start;
                        (*ptr).size+=size;
                        return;
                    }else {
                        let mut placeholder = Box::new(Node { start: start, size: size, next: None });
                        ::core::mem::swap(&mut (placeholder.next), &mut (*iterator));
                        *iterator = Some(placeholder); //releasing a None.
                    }
                }else{
                    iterator=&mut ((*pos.unwrap()).next) as Slot;
                    pos=next_addr(iterator);
                }
            }
        }
    }
    */
}
