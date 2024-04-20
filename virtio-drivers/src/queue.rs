use crate::error::{VirtIoError, VirtIoResult};
use crate::hal::QueuePage;
use crate::{PhysAddr, PAGE_SIZE};
use alloc::boxed::Box;
use alloc::collections::{BTreeSet, VecDeque};
use alloc::vec::Vec;
use core::mem::size_of;
use core::sync::atomic::{fence, Ordering};
use log::info;

/// Align `size` up to a page.
const fn align_up(size: usize) -> usize {
    (size + PAGE_SIZE) & !(PAGE_SIZE - 1)
}

pub struct VirtIoQueue<const SIZE: usize> {
    queue_page: Box<dyn QueuePage<SIZE>>,
    desc_pa: PhysAddr,
    // real queue, storage available descriptor indexes
    avail_desc_index: VecDeque<u16>,
    last_seen_used: u16,
    poped_used: BTreeSet<u16>,
}

impl<const SIZE: usize> VirtIoQueue<SIZE> {
    const AVAIL_RING_OFFSET: usize = size_of::<Descriptor>() * SIZE;
    const DESCRIPTOR_TABLE_OFFSET: usize = 0;
    const USED_RING_OFFSET: usize =
        align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>());

    pub fn new(mut queue_page: Box<dyn QueuePage<SIZE>>) -> Self {
        let desc_pa = queue_page
            .as_descriptor_table_at(Self::DESCRIPTOR_TABLE_OFFSET)
            .as_ptr() as usize;
        queue_page.as_mut_avail_ring(Self::AVAIL_RING_OFFSET).init();
        let avail_desc_index = VecDeque::from_iter(0..SIZE as u16);
        VirtIoQueue {
            queue_page,
            desc_pa,
            avail_desc_index,
            last_seen_used: 0,
            poped_used: BTreeSet::new(),
        }
    }
    pub(crate) fn desc_pa(&self) -> PhysAddr {
        self.desc_pa
    }
    fn avail_ring_pa(&self) -> PhysAddr {
        self.desc_pa + size_of::<Descriptor>() * SIZE
    }
    fn used_ring_pa(&self) -> PhysAddr {
        self.desc_pa + align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>())
    }

    pub(crate) const fn total_size() -> usize {
        align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>())
            + align_up(size_of::<UsedRing<SIZE>>())
    }

    pub(crate) fn push(&mut self, mut data: Vec<Descriptor>) -> VirtIoResult<u16> {
        assert_ne!(data.len(), 0);
        if self.avail_desc_index.len() < data.len() {
            return Err(VirtIoError::QueueFull);
        }
        let mut last = None;
        let desc = self
            .queue_page
            .as_mut_descriptor_table_at(Self::DESCRIPTOR_TABLE_OFFSET);
        let avail_ring = self.queue_page.as_mut_avail_ring(Self::AVAIL_RING_OFFSET);
        for d in data.iter_mut().rev() {
            let id = self.avail_desc_index.pop_front().unwrap();
            if let Some(nex) = last {
                d.next = nex;
            }
            desc[id as usize % SIZE] = *d;
            last = Some(id);
        }
        let head = last.unwrap();
        // change the avail ring
        avail_ring.push(head)?;
        // Write barrier so that device sees changes to descriptor table and available ring before
        // change to available index.
        fence(Ordering::SeqCst);
        Ok(head)
    }

    pub(crate) fn can_pop(&self) -> VirtIoResult<bool> {
        fence(Ordering::SeqCst);
        let used_ring = self.queue_page.as_used_ring(Self::USED_RING_OFFSET);
        if self.last_seen_used != used_ring.idx {
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn pop(&mut self, id: u16) -> VirtIoResult<()> {
        let used_ring = self.queue_page.as_mut_used_ring(Self::USED_RING_OFFSET);
        let desc = self
            .queue_page
            .as_descriptor_table_at(Self::DESCRIPTOR_TABLE_OFFSET);
        assert_ne!(self.last_seen_used, used_ring.idx);
        let mut header = self.last_seen_used.wrapping_sub(1);
        let skip = used_ring.idx.wrapping_sub(self.last_seen_used);
        let mut tmp_index = self.last_seen_used;
        for _ in 0..skip {
            if used_ring.ring[tmp_index as usize % SIZE].id == id as u32 {
                header = tmp_index;
                break;
            }
            tmp_index = tmp_index.wrapping_add(1);
        }
        // make sure we find the header
        assert_ne!(header, self.last_seen_used.wrapping_sub(1));
        self.poped_used.insert(header);
        let mut now = used_ring.ring[header as usize % SIZE].id as usize;
        self.avail_desc_index.push_back(now as _);
        while (desc[now].flags & DescFlag::NEXT) != 0 {
            now = desc[now % SIZE].next as _;
            self.avail_desc_index.push_back(now as _);
        }
        // update last_seen_used
        while self.poped_used.contains(&self.last_seen_used) {
            self.poped_used.remove(&self.last_seen_used);
            self.last_seen_used += 1;
        }
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
    }
    fn push(&mut self, id: u16) -> VirtIoResult<u16> {
        // have enough space, because (avail ring's len == desc's)
        self.ring[self.idx as usize % SIZE] = id;
        let res = self.idx;
        self.idx = self.idx.wrapping_add(1);
        Ok(res)
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
