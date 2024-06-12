use crate::error::{VirtIoError, VirtIoResult};
use crate::hal::{Hal, QueuePage};
use crate::transport::Transport;
use crate::{align_up, pages};
use alloc::boxed::Box;
use alloc::collections::{BTreeSet, VecDeque};
use alloc::vec::Vec;
use core::hint::spin_loop;
use core::marker::PhantomData;
use core::mem::size_of;
use core::sync::atomic::{fence, AtomicU16, Ordering};

pub struct VirtIoQueue<H: Hal<SIZE>, const SIZE: usize> {
    queue_page: Box<dyn QueuePage<SIZE>>,
    queue_ref: QueueMutRef<SIZE>,
    // storage available descriptor indexes
    avail_desc_index: VecDeque<u16>,
    last_seen_used: u16,
    poped_used: BTreeSet<u16>,
    /// The index of queue
    queue_idx: u16,
    _hal: PhantomData<H>,
}

impl<H: Hal<SIZE>, const SIZE: usize> VirtIoQueue<H, SIZE> {
    const AVAIL_RING_OFFSET: usize = size_of::<Descriptor>() * SIZE;
    const DESCRIPTOR_TABLE_OFFSET: usize = 0;
    const USED_RING_OFFSET: usize =
        align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>());

    pub fn new<T: Transport>(transport: &mut T, queue_idx: u16) -> VirtIoResult<Self> {
        if transport.queue_used(queue_idx)? {
            return Err(VirtIoError::AlreadyUsed);
        }
        if !SIZE.is_power_of_two()
            || SIZE > u16::MAX.into()
            || transport.max_queue_size(queue_idx)? < SIZE as u32
        {
            return Err(VirtIoError::InvalidParam);
        }
        let size = SIZE as u16;
        let mut queue_page = H::dma_alloc(pages(Self::total_size()));
        let descriptors_paddr = queue_page.paddr();
        let driver_area_paddr = descriptors_paddr + Self::AVAIL_RING_OFFSET;
        let device_area_paddr = descriptors_paddr + Self::USED_RING_OFFSET;
        transport.queue_set(
            queue_idx,
            size as _,
            descriptors_paddr,
            driver_area_paddr,
            device_area_paddr,
        )?;
        let avail_desc_index = VecDeque::from_iter(0..SIZE as u16);
        let queue_ref_mut = queue_page.queue_ref_mut(&QueueLayout::new::<SIZE>());
        Ok(VirtIoQueue {
            queue_page,
            queue_idx,
            queue_ref: queue_ref_mut,
            avail_desc_index,
            last_seen_used: 0,
            poped_used: BTreeSet::new(),
            _hal: PhantomData,
        })
    }

    const fn total_size() -> usize {
        align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>())
            + align_up(size_of::<UsedRing<SIZE>>())
    }

    /// Add the given buffers to the virtqueue, notifies the device, blocks until the device uses
    /// them, then pops them.
    ///
    /// This assumes that the device isn't processing any other buffers at the same time.
    ///
    /// The buffers must not be empty.
    pub fn add_notify_wait_pop<T: Transport>(
        &mut self,
        transport: &mut T,
        descriptors: Vec<Descriptor>,
    ) -> VirtIoResult<u32> {
        let token = self.add(descriptors)?;
        // Notify the queue.
        if self.should_notify() {
            transport.notify(self.queue_idx)?;
        }
        // Wait until there is at least one element in the used ring.
        while !self.can_pop(token)? {
            spin_loop();
        }
        self.pop_used(token)
    }

    /// Returns whether the driver should notify the device after adding a new buffer to the
    /// virtqueue.
    ///
    /// This will be false if the device has supressed notifications.
    pub fn should_notify(&self) -> bool {
        // if self.event_idx {
        //     // instance of UsedRing.
        //     let avail_event = unsafe { (*self.used.as_ptr()).avail_event };
        //     self.avail_idx >= avail_event.wrapping_add(1)
        // } else {
        //     // instance of UsedRing.
        //     unsafe { (*self.used.as_ptr()).flags & 0x0001 == 0 }
        // }
        self.queue_ref.used_ring.flags.load(Ordering::Acquire) & 0x0001 == 0
    }

    /// Add buffers to the virtqueue, return a token.
    ///
    /// The buffers must not be empty.
    ///
    /// Ref: linux virtio_ring.c virtqueue_add
    ///
    /// # Safety
    ///
    /// The input and output buffers must remain valid and not be accessed until a call to
    /// `pop_used` with the returned token succeeds.
    pub(super) fn add(&mut self, data: Vec<Descriptor>) -> VirtIoResult<u16> {
        assert_ne!(data.len(), 0);
        if self.avail_desc_index.len() < data.len() {
            return Err(VirtIoError::QueueFull);
        }
        let mut last = None;
        let desc = &mut self.queue_ref.descriptor_table;
        let avail_ring = &mut self.queue_ref.avail_ring;
        for mut d in data.into_iter().rev() {
            let id = self.avail_desc_index.pop_front().unwrap();
            if let Some(nex) = last {
                d.next = nex;
            }
            desc[id as usize % SIZE] = d;
            last = Some(id);
        }
        fence(Ordering::SeqCst);
        let head = last.unwrap();
        // change the avail ring
        avail_ring.push(head)?;
        Ok(head)
    }

    pub(crate) fn can_pop(&self, id: u16) -> VirtIoResult<bool> {
        let used_ring = &self.queue_ref.used_ring;
        let idx = used_ring.idx.load(Ordering::Acquire);
        if self.last_seen_used == idx {
            return Ok(false);
        }
        let skip = idx.wrapping_sub(self.last_seen_used);
        let mut current_index = self.last_seen_used;
        for _ in 0..skip {
            if used_ring.ring[current_index as usize % SIZE].id == id as u32 {
                return Ok(true);
            }
            current_index = current_index.wrapping_add(1);
        }
        Ok(false)
    }
    /// Returns the descriptor index (a.k.a. token) of the next used element without popping it, or
    /// `None` if the used ring is empty.
    pub(crate) fn peek_used(&self) -> Option<u16> {
        let used_ring = &self.queue_ref.used_ring;
        if self.last_seen_used == used_ring.idx.load(Ordering::Acquire) {
            return None;
        }
        let id = used_ring.ring[self.last_seen_used as usize % SIZE].id;
        Some(id as _)
    }

    pub fn get_desc_len(&self, id: u16) -> usize {
        let descs = &self.queue_ref.descriptor_table;
        descs[id as usize].len as _
    }

    /// Returns the number of free descriptors.
    pub fn available_desc(&self) -> usize {
        // #[cfg(feature = "alloc")]
        // if self.indirect {
        //     return if usize::from(self.num_used) == SIZE {
        //         0
        //     } else {
        //         SIZE
        //     };
        // }
        self.avail_desc_index.len()
    }

    /// If the given token is next on the device used queue, pops it and returns the total buffer
    /// length which was used (written) by the device.
    ///
    /// Ref: linux virtio_ring.c virtqueue_get_buf_ctx
    ///
    /// # Safety
    ///
    /// The buffers in `inputs` and `outputs` must match the set of buffers originally added to the
    /// queue by `add` when it returned the token being passed in here.
    pub(crate) fn pop_used(&mut self, id: u16) -> VirtIoResult<u32> {
        if !self.can_pop(id)? {
            return Err(VirtIoError::NotReady);
        }
        let used_ring = &mut self.queue_ref.used_ring;
        let desc = &self.queue_ref.descriptor_table;
        let idx = used_ring.idx.load(Ordering::Acquire);
        assert_ne!(self.last_seen_used, idx);
        let mut header = self.last_seen_used.wrapping_sub(1);
        let skip = idx.wrapping_sub(self.last_seen_used);
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

        let mut now = id as usize;
        // todo!(fix it)
        let len = used_ring.ring[header as usize % SIZE].len;
        self.avail_desc_index.push_back(now as _);
        while (desc[now].flags & DescFlag::NEXT) != 0 {
            now = desc[now % SIZE].next as _;
            self.avail_desc_index.push_back(now as _);
        }
        // update last_seen_used
        while self.poped_used.contains(&self.last_seen_used) {
            self.poped_used.remove(&self.last_seen_used);
            self.last_seen_used = self.last_seen_used.wrapping_add(1);
            self.queue_ref
                .avail_ring
                .used_event
                .store(self.last_seen_used, Ordering::Release);
        }
        Ok(len)
    }
}

pub struct QueueLayout {
    pub descriptor_table_offset: usize,
    pub avail_ring_offset: usize,
    pub used_ring_offset: usize,
}

impl QueueLayout {
    pub fn new<const SIZE: usize>() -> Self {
        Self {
            descriptor_table_offset: 0,
            avail_ring_offset: size_of::<Descriptor>() * SIZE,
            used_ring_offset: align_up(
                size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>(),
            ),
        }
    }
}

pub struct QueueMutRef<const SIZE: usize> {
    pub descriptor_table: &'static mut [Descriptor],
    pub avail_ring: &'static mut AvailRing<SIZE>,
    pub used_ring: &'static mut UsedRing<SIZE>,
}

#[repr(C, align(16))]
#[derive(Debug)]
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
    pub(crate) fn new<const SIZE: usize, H: Hal<SIZE>>(vaddr: usize, len: u32, flags: u16) -> Self {
        Self {
            addr: H::to_paddr(vaddr) as _,
            len,
            flags,
            next: 0,
        }
    }
}
pub struct DescFlag;
impl DescFlag {
    pub(crate) const EMPTY: u16 = 0;
    pub(crate) const NEXT: u16 = 1;
    pub(crate) const WRITE: u16 = 2;
    const INDIRECT: u16 = 4;
}
#[repr(C)]
#[derive(Debug)]
pub struct AvailRing<const SIZE: usize> {
    flags: AtomicU16,
    /// A driver MUST NOT decrement the idx.
    idx: AtomicU16,
    ring: [u16; SIZE],
    /// Only used if `VIRTIO_F_EVENT_IDX` is negotiated.
    used_event: AtomicU16,
}
impl<const SIZE: usize> AvailRing<SIZE> {
    fn push(&mut self, id: u16) -> VirtIoResult<u16> {
        // have enough space, because (avail ring's len == desc's)
        let res = self.idx.load(Ordering::Acquire);
        self.ring[res as usize % SIZE] = id;
        self.idx.store(res.wrapping_add(1), Ordering::Release);
        Ok(res)
    }
}
#[repr(C)]
#[derive(Debug)]
pub struct UsedRing<const SIZE: usize> {
    flags: AtomicU16,
    idx: AtomicU16,
    ring: [UsedElem; SIZE],
    /// Only used if `VIRTIO_F_EVENT_IDX` is negotiated.
    avail_event: AtomicU16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UsedElem {
    id: u32,
    len: u32,
}
