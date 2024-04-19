use alloc::boxed::Box;
use alloc::collections::{BTreeSet, VecDeque};
use alloc::vec::Vec;
use core::mem::size_of;
use crate::{PAGE_SIZE, PhysAddr};
use crate::error::{VirtIoError, VirtIoResult};
use crate::hal::{QueuePage};


/// Align `size` up to a page.
const fn align_up(size: usize) -> usize {
    (size + PAGE_SIZE) & !(PAGE_SIZE - 1)
}


pub struct VirtIoQueue<const SIZE: usize>{
    queue_page: Box<dyn QueuePage<SIZE>>,
    desc_pa: PhysAddr,
    // real queue, storage available descriptor indexes
    q: VecDeque<u16>,
    last_seen_used: u16,
    poped_used: BTreeSet<u16>,
}


impl <const SIZE:usize> VirtIoQueue<SIZE>{
    const AVAIL_RING_OFFSET:usize = size_of::<Descriptor>() * SIZE;
    const DESCRIPTOR_TABLE_OFFSET:usize = 0;
    const USED_RING_OFFSET:usize = align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>());

    pub fn new(mut queue_page:Box<dyn QueuePage<SIZE>>) ->Self{
        let desc_pa = queue_page.as_descriptor_table_at(Self::DESCRIPTOR_TABLE_OFFSET).as_ptr() as usize;
        queue_page.as_mut_avail_ring(Self::AVAIL_RING_OFFSET).init();
        let q = VecDeque::from_iter(0..SIZE as u16);
        VirtIoQueue{
            queue_page,
            desc_pa,
            q,
            last_seen_used: 0,
            poped_used: BTreeSet::new(),
        }
    }
    pub(crate) fn desc_pa(&self) -> PhysAddr {
        self.desc_pa
    }
    fn avail_ring_pa(&self) -> PhysAddr {
        // don't use `Self::avail_offset` for speed
        self.desc_pa + size_of::<Descriptor>() * SIZE
    }
    fn used_ring_pa(&self) -> PhysAddr {
        // don't use `Self::used_offset` for speed
        self.desc_pa + align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>())
    }

    pub(crate) const fn total_size() -> usize {
        align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>())
            + align_up(size_of::<UsedRing<SIZE>>())
    }

    pub (crate) fn push(&mut self, mut data: Vec<Descriptor>) -> VirtIoResult<u16> {
        assert_ne!(data.len(), 0);
        if self.q.len() < data.len() {
            return Err(VirtIoError::QueueFull)
        }
        let mut last = None;
        let desc = self.queue_page.as_mut_descriptor_table_at(Self::DESCRIPTOR_TABLE_OFFSET);
        let avail_ring = self.queue_page.as_mut_avail_ring(Self::AVAIL_RING_OFFSET);
        for d in data.iter_mut().rev() {
            let id = self.q.pop_front().unwrap();
            if let Some(nex) = last {
                d.next = nex;
            }
            // warn!(
            //     "buffer len : {} id={} nex_flag={}, nex={}",
            //     d.len,
            //     id,
            //     d.flags & DescFlag::NEXT,
            //     d.next
            // );
            //  write desc to self.desc
            desc[id as usize % SIZE] = *d;
            last = Some(id);
        }
        let head = last.unwrap();
        // change the avail ring
        avail_ring.push(head)?;
        Ok(head)
    }

    pub(crate) fn is_ready(&self, id: u16) -> VirtIoResult<bool> {
        let used_ring = self.queue_page.as_used_ring(Self::USED_RING_OFFSET);
        if self.last_seen_used == used_ring.idx {
            return Ok(false);
        }
        for i in self.last_seen_used..used_ring.idx {
            if used_ring.ring[i as usize % SIZE].id == id as u32 {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn pop(&mut self, id: u16) -> VirtIoResult<()> {
        let used_ring = self.queue_page.as_mut_used_ring(Self::USED_RING_OFFSET);
        let desc = self.queue_page.as_descriptor_table_at(Self::DESCRIPTOR_TABLE_OFFSET);
        assert!(self.last_seen_used < used_ring.idx);
        let mut header = self.last_seen_used - 1;
        for i in self.last_seen_used..used_ring.idx {
            if used_ring.ring[i as usize % SIZE].id == id as u32 {
                // pop this
                header = i;
                break;
            }
        }
        assert_ne!(header, self.last_seen_used - 1);
        self.poped_used.insert(header);
        let mut now = used_ring.ring[header as usize % SIZE].id as usize;
        self.q.push_back(now as _);
        while (desc[now].flags & DescFlag::NEXT) != 0 {
            now = desc[now as usize % SIZE].next as _;
            self.q.push_back(now as _);
        }
        // update last_seen_used
        while self.poped_used.contains(&self.last_seen_used) {
            self.poped_used.remove(&self.last_seen_used);
            self.last_seen_used += 1;
        }
        // return value
        Ok(())
    }

}

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}
impl Default for Descriptor {
    fn default() -> Self {
        Self {
            addr: Default::default(),
            len: Default::default(),
            flags: Default::default(),
            next: Default::default(),
        }
    }
}
impl Descriptor {
   pub(crate) fn new(addr: u64, len: u32, flags: u16) -> Self {
        Self {
            addr,
            len,
            flags,
            next: 0,
        }
    }
}
pub struct DescFlag;
impl DescFlag {
    pub(crate) const NEXT: u16 = 1;
    pub(crate) const WRITE: u16 = 2;
    const INDIRECT: u16 = 4;
}
#[repr(C)]
#[derive(Debug)]
pub struct AvailRing<const SIZE: usize> {
    flags: u16,
    /// A driver MUST NOT decrement the idx.
    idx: u16,
    ring: [u16; SIZE],
    /// Only used if `VIRTIO_F_EVENT_IDX` is negotiated.
    used_event: u16,
}
impl<const SIZE: usize> AvailRing<SIZE> {
    fn init(&mut self) {
        self.flags = 0;
        self.idx = 0;
        // for i in 0..SIZE {
        //     self.ring[i] = i as u16;
        // }
        // self.used_event = 0;
    }
    fn push(&mut self, id: u16) -> VirtIoResult<u16> {
        // have enough space, because (avail ring's len == desc's)
        self.ring[self.idx as usize % SIZE] = id;
        // warn!(
        //     "avail pushed: id={} header={}",
        //     self.idx,
        //     self.ring[self.idx as usize % SIZE]
        // );
        self.idx = self.idx + 1;
        Ok(self.idx - 1)
    }
}
#[repr(C)]
#[derive(Debug)]
pub struct UsedRing<const SIZE: usize> {
    flags: u16,
    idx: u16,
    ring: [UsedElem; SIZE],
    /// Only used if `VIRTIO_F_EVENT_IDX` is negotiated.
    avail_event: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UsedElem {
    id: u32,
    len: u32,
}