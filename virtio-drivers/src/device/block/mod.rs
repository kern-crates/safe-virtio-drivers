use crate::error::{VirtIoError, VirtIoResult};
use crate::hal::{Hal, VirtIoDeviceIo};
use crate::queue::{DescFlag, Descriptor, VirtIoQueue};
use crate::transport::mmio::VirtIOHeader;
use crate::volatile::{ReadVolatile, WriteVolatile};
use crate::{pages, PAGE_SIZE};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::hint::spin_loop;
use core::marker::PhantomData;
use core::mem::size_of_val;
use log::{info, warn};
use ty::*;
mod ty;

const SUPPORT_FEAT: BlkFeature = BlkFeature::FLUSH;
const QUEUE_SIZE: usize = 16;
pub const SECTOR_SIZE: usize = 512;

pub struct VirtIOBlk<H: Hal<QUEUE_SIZE>> {
    io_region: Box<dyn VirtIoDeviceIo>,
    queue: VirtIoQueue<QUEUE_SIZE>,
    capacity: u64,
    negotiated_features: BlkFeature,
    _hal: PhantomData<H>,
}

impl<H: Hal<QUEUE_SIZE>> VirtIOBlk<H> {
    pub fn new(io_region: Box<dyn VirtIoDeviceIo>) -> VirtIoResult<Self> {
        let header = VirtIOHeader::default();
        header.general_init(&io_region, SUPPORT_FEAT.bits())?;
        // read config
        let config = BlkConfig::default();
        let capacity = ((config.capacity_high.read_volatile_u32(&io_region)? as u64) << 32)
            | (config.capacity_low.read_volatile_u32(&io_region)? as u64);
        info!("block device size: {}KB {}sector", capacity / 2, capacity);
        // set queue
        let queue_page = H::dma_alloc(pages(VirtIoQueue::<QUEUE_SIZE>::total_size()));
        let queue = VirtIoQueue::new(queue_page);
        if header.is_legacy(&io_region)? {
            let size = QUEUE_SIZE;
            let align = PAGE_SIZE as u32;
            let pfn = (queue.desc_pa() / PAGE_SIZE) as u32;
            // if desc_pa can be divided by PAGE_SIZE
            assert_eq!(pfn as usize * PAGE_SIZE, queue.desc_pa());
            // queue index
            header.queue_sel.write_volatile_u32(0, &io_region)?;
            let qm = header.queue_num_max.read_volatile_u32(&io_region)? as usize;
            if qm < size {
                return Err(VirtIoError::InvalidParam);
            }
            header
                .queue_num
                .write_volatile_u32(size as u32, &io_region)?;
            header
                .legacy_queue_align
                .write_volatile_u32(align, &io_region)?;
            header
                .legacy_queue_pfn
                .write_volatile_u32(pfn, &io_region)?;
        } else {
            // modern interface
            todo!("modern interface do not implement yet. please use the legacy instead.");
        }
        header.general_init_end(&io_region)?;
        Ok(Self {
            io_region,
            queue,
            capacity,
            negotiated_features: SUPPORT_FEAT,
            _hal: PhantomData,
        })
    }
    /// assert_eq!(buf.len() % 512, 0)
    pub fn read_blocks(&mut self, sector: usize, buf: &mut [u8]) -> VirtIoResult<()> {
        assert_ne!(buf.len(), 0);
        assert_eq!(buf.len() % SECTOR_SIZE, 0);
        let header = VirtIOHeader::default();
        let ops = &self.io_region;

        let mut desc_vec = Vec::new();
        let req = BlkReq::new(BlkReqType::In, sector as u64);
        let resp = BlkRespStatus::default();
        // get the physical address of header
        desc_vec.push(Descriptor::new(
            &req as *const _ as _,
            size_of_val(&req) as _,
            DescFlag::NEXT,
        ));
        desc_vec.push(Descriptor::new(
            buf.as_ptr() as _,
            buf.len() as _,
            DescFlag::NEXT | DescFlag::WRITE,
        ));
        desc_vec.push(Descriptor::new(
            &resp as *const _ as _,
            size_of_val(&resp) as _,
            DescFlag::WRITE,
        ));
        let token = self.queue.push(desc_vec)?;
        // notify the device
        header.queue_notify.write_volatile_u32(0, ops)?;
        while !self.queue.can_pop()? {
            spin_loop();
        }
        self.queue.pop(token)?;
        debug_assert_eq!(resp, BlkRespStatus::OK);
        resp.into()
    }
    /// assert_eq!(buf.len() % 512, 0)
    pub fn write_blocks(&mut self, sector: usize, buf: &[u8]) -> VirtIoResult<()> {
        assert_ne!(buf.len(), 0);
        assert_eq!(buf.len() % SECTOR_SIZE, 0);
        let header = VirtIOHeader::default();
        let ops = &self.io_region;
        let mut v = Vec::new();
        let req = BlkReq::new(BlkReqType::Out, sector as u64);
        let resp = BlkRespStatus::default();
        // get the physical address of header
        v.push(Descriptor::new(
            &req as *const _ as _,
            size_of_val(&req) as _,
            DescFlag::NEXT,
        ));
        v.push(Descriptor::new(
            buf.as_ptr() as _,
            buf.len() as _,
            DescFlag::NEXT,
        ));
        v.push(Descriptor::new(
            &resp as *const _ as _,
            size_of_val(&resp) as _,
            DescFlag::WRITE,
        ));
        let token = self.queue.push(v)?;
        // notify the device
        header.queue_notify.write_volatile_u32(0, ops)?;
        while !self.queue.can_pop()? {
            spin_loop();
        }
        self.queue.pop(token)?;
        debug_assert_eq!(resp, BlkRespStatus::OK);
        resp.into()
    }
    pub fn capacity(&self) -> VirtIoResult<u64> {
        Ok(self.capacity)
    }
    pub fn flush(&mut self) -> VirtIoResult<()> {
        if self.negotiated_features.contains(BlkFeature::FLUSH) {
            self.request(BlkReq::new(BlkReqType::Flush, 0))
        } else {
            Ok(())
        }
    }
    /// Sends the given request to the device and waits for a response, with no extra data.
    fn request(&mut self, request: BlkReq) -> VirtIoResult<()> {
        let mut resp = BlkRespStatus::default();
        let desc_vec = vec![
            Descriptor::new(
                &request as *const _ as _,
                size_of_val(&request) as _,
                DescFlag::NEXT,
            ),
            Descriptor::new(
                &resp as *const _ as _,
                size_of_val(&resp) as _,
                DescFlag::WRITE,
            ),
        ];
        let token = self.queue.push(desc_vec)?;
        let header = VirtIOHeader::default();
        header.queue_notify.write_volatile_u32(0, &self.io_region)?;
        while !self.queue.can_pop()? {
            spin_loop();
        }
        self.queue.pop(token)?;
        debug_assert_eq!(resp, BlkRespStatus::OK);
        resp.into()
    }
}
