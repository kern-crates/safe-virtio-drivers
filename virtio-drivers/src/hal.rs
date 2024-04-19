use alloc::boxed::Box;
use crate::error::VirtIoResult;
use crate::queue::{AvailRing, Descriptor,UsedRing};

pub trait VirtIoDeviceIo: Send + Sync {
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32>;
    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()>;
}

pub trait QueuePage<const SIZE: usize>{
    fn as_descriptor_table_at<'a>(&self, offset: usize) -> &'a [Descriptor];
    fn as_mut_descriptor_table_at<'a>(&mut self, offset: usize) -> &'a mut [Descriptor];
    fn as_avail_ring_at<'a>(&self, offset:usize ) -> &'a AvailRing<SIZE>;
    fn as_mut_avail_ring<'a>(&mut self, offset:usize) -> &'a mut AvailRing<SIZE>;
    fn as_used_ring<'a>(&self,offset:usize) -> &'a UsedRing<SIZE>;
    fn as_mut_used_ring<'a>(&mut self, offset:usize) -> &'a mut UsedRing<SIZE>;
}

pub trait Hal<const SIZE: usize>{
    fn dma_alloc(pages:usize)->Box<dyn QueuePage<SIZE>>;
}
