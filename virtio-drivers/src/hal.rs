use crate::error::VirtIoResult;
use crate::queue::{AvailRing, Descriptor, UsedRing};
use crate::{PhysAddr, VirtAddr};
use alloc::boxed::Box;
use core::fmt::Debug;

pub trait VirtIoDeviceIo: Send + Sync + Debug {
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32>;
    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()>;
}

impl VirtIoDeviceIo for Box<dyn VirtIoDeviceIo> {
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32> {
        self.as_ref().read_volatile_u32_at(off)
    }

    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()> {
        self.as_ref().write_volatile_u32_at(off, data)
    }
}

pub trait QueuePage<const SIZE: usize>: Send + Sync {
    fn as_descriptor_table_at<'a>(&self, offset: usize) -> &'a [Descriptor];
    fn as_mut_descriptor_table_at<'a>(&mut self, offset: usize) -> &'a mut [Descriptor];
    fn as_avail_ring_at<'a>(&self, offset: usize) -> &'a AvailRing<SIZE>;
    fn as_mut_avail_ring<'a>(&mut self, offset: usize) -> &'a mut AvailRing<SIZE>;
    fn as_used_ring<'a>(&self, offset: usize) -> &'a UsedRing<SIZE>;
    fn as_mut_used_ring<'a>(&mut self, offset: usize) -> &'a mut UsedRing<SIZE>;
    fn vaddr(&self) -> PhysAddr;
    fn paddr(&self) -> VirtAddr;
}

pub trait Hal<const SIZE: usize>: Send + Sync {
    fn dma_alloc(pages: usize) -> Box<dyn QueuePage<SIZE>>;
}

/// The direction in which a buffer is passed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BufferDirection {
    /// The buffer may be read or written by the driver, but only read by the device.
    DriverToDevice,
    /// The buffer may be read or written by the device, but only read by the driver.
    DeviceToDriver,
    /// The buffer may be read or written by both the device and the driver.
    Both,
}
