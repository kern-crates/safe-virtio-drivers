use crate::error::VirtIoResult;
use crate::hal::Hal;
use crate::queue::{DescFlag, Descriptor, VirtIoQueue};

use crate::volatile::ReadVolatile;

use alloc::vec;

use crate::transport::Transport;
use core::mem::size_of_val;

use log::info;
use ty::*;

mod ty;

const SUPPORTED_FEATURES: BlkFeature = BlkFeature::FLUSH;
const QUEUE_SIZE: usize = 16;
pub const SECTOR_SIZE: usize = 512;

pub struct VirtIOBlk<H: Hal<QUEUE_SIZE>, T: Transport> {
    transport: T,
    queue: VirtIoQueue<H, QUEUE_SIZE>,
    capacity: u64,
    negotiated_features: BlkFeature,
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> VirtIOBlk<H, T> {
    /// Create a new VirtIO-Blk driver.
    pub fn new(mut transport: T) -> VirtIoResult<Self> {
        let negotiated_features = transport.begin_init(SUPPORTED_FEATURES)?;
        let io_region = transport.io_region();
        // read config
        let config = BlkConfig::default();
        let capacity = ((config.capacity_high.read(io_region)? as u64) << 32)
            | (config.capacity_low.read(io_region)? as u64);
        info!("block device size: {}KB", capacity / 2);
        let queue = VirtIoQueue::new(&mut transport, 0)?;
        transport.finish_init()?;
        Ok(Self {
            transport,
            queue,
            capacity,
            negotiated_features,
        })
    }

    /// Gets the capacity of the block device, in 512 byte ([`SECTOR_SIZE`]) sectors.
    pub fn capacity(&self) -> VirtIoResult<u64> {
        Ok(self.capacity)
    }

    /// Returns true if the block device is read-only, or false if it allows writes.
    pub fn readonly(&self) -> bool {
        self.negotiated_features.contains(BlkFeature::RO)
    }

    /// Acknowledges a pending interrupt, if any.
    ///
    /// Returns true if there was an interrupt to acknowledge.
    pub fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        self.transport.ack_interrupt()
    }

    /// Sends the given request to the device and waits for a response, including the given data.
    fn request_read(&mut self, request: BlkReq, data: &mut [u8]) -> VirtIoResult<()> {
        let resp = BlkRespStatus::default();
        let req = Descriptor::new::<QUEUE_SIZE, H>(
            &request as *const _ as _,
            size_of_val(&request) as _,
            DescFlag::NEXT,
        );
        let data = Descriptor::new::<QUEUE_SIZE, H>(
            data.as_ptr() as _,
            data.len() as _,
            DescFlag::NEXT | DescFlag::WRITE,
        );
        let res = Descriptor::new::<QUEUE_SIZE, H>(
            &resp as *const _ as _,
            size_of_val(&resp) as _,
            DescFlag::WRITE,
        );
        self.queue
            .add_notify_wait_pop(&mut self.transport, vec![req, data, res])?;
        debug_assert_eq!(resp, BlkRespStatus::OK);
        resp.into()
    }

    /// Sends the given request and data to the device and waits for a response.
    fn request_write(&mut self, request: BlkReq, data: &[u8]) -> VirtIoResult<()> {
        let resp = BlkRespStatus::default();
        let req = Descriptor::new::<QUEUE_SIZE, H>(
            &request as *const _ as _,
            size_of_val(&request) as _,
            DescFlag::NEXT,
        );
        let data =
            Descriptor::new::<QUEUE_SIZE, H>(data.as_ptr() as _, data.len() as _, DescFlag::NEXT);
        let res = Descriptor::new::<QUEUE_SIZE, H>(
            &resp as *const _ as _,
            size_of_val(&resp) as _,
            DescFlag::WRITE,
        );
        let _len = self
            .queue
            .add_notify_wait_pop(&mut self.transport, vec![req, data, res])?;
        debug_assert_eq!(resp, BlkRespStatus::OK);
        resp.into()
    }

    /// Gets the device ID.
    ///
    /// The ID is written as ASCII into the given buffer, which must be 20 bytes long, and the used
    /// length returned.
    pub fn device_id(&mut self, id: &mut [u8; 20]) -> VirtIoResult<usize> {
        self.request_read(BlkReq::new(BlkReqType::GetId, 0), id)?;
        let length = id.iter().position(|&x| x == 0).unwrap_or(20);
        Ok(length)
    }

    /// Reads one or more blocks into the given buffer.
    ///
    /// The buffer length must be a non-zero multiple of [`SECTOR_SIZE`].
    ///
    /// Blocks until the read completes or there is an error.
    pub fn read_blocks(&mut self, sector: usize, buf: &mut [u8]) -> VirtIoResult<()> {
        assert_ne!(buf.len(), 0);
        assert_eq!(buf.len() % SECTOR_SIZE, 0);
        self.request_read(BlkReq::new(BlkReqType::In, sector as u64), buf)
    }
    /// assert_eq!(buf.len() % 512, 0)
    pub fn write_blocks(&mut self, sector: usize, buf: &[u8]) -> VirtIoResult<()> {
        assert_ne!(buf.len(), 0);
        assert_eq!(buf.len() % SECTOR_SIZE, 0);
        self.request_write(BlkReq::new(BlkReqType::Out, sector as u64), buf)
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
        let resp = BlkRespStatus::default();
        let desc_vec = vec![
            Descriptor::new::<QUEUE_SIZE, H>(
                &request as *const _ as _,
                size_of_val(&request) as _,
                DescFlag::NEXT,
            ),
            Descriptor::new::<QUEUE_SIZE, H>(
                &resp as *const _ as _,
                size_of_val(&resp) as _,
                DescFlag::WRITE,
            ),
        ];
        self.queue
            .add_notify_wait_pop(&mut self.transport, desc_vec)?;
        debug_assert_eq!(resp, BlkRespStatus::OK);
        resp.into()
    }
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> Drop for VirtIOBlk<H, T> {
    fn drop(&mut self) {
        self.transport
            .queue_unset(0)
            .expect("failed to unset queue");
    }
}
