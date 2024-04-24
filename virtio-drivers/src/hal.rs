use crate::error::VirtIoResult;
use crate::queue::{AvailRing, Descriptor, UsedRing};
use crate::{PhysAddr, VirtAddr};
use alloc::boxed::Box;
use core::fmt::Debug;

pub trait VirtIoDeviceIo: Send + Sync + Debug {
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32>;
    fn read_volatile_u8_at(&self, off: usize) -> VirtIoResult<u8>;
    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()>;
    fn write_volatile_u8_at(&self, off: usize, data: u8) -> VirtIoResult<()>;
    fn paddr(&self) -> PhysAddr;
    fn vaddr(&self) -> VirtAddr;
}

impl VirtIoDeviceIo for Box<dyn VirtIoDeviceIo> {
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32> {
        self.as_ref().read_volatile_u32_at(off)
    }
    fn read_volatile_u8_at(&self, off: usize) -> VirtIoResult<u8> {
        self.as_ref().read_volatile_u8_at(off)
    }
    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()> {
        self.as_ref().write_volatile_u32_at(off, data)
    }
    fn write_volatile_u8_at(&self, off: usize, data: u8) -> VirtIoResult<()> {
        self.as_ref().write_volatile_u8_at(off, data)
    }
    fn paddr(&self) -> PhysAddr {
        self.as_ref().paddr()
    }

    fn vaddr(&self) -> VirtAddr {
        self.as_ref().vaddr()
    }
}

pub trait DevicePage: Send + Sync {
    fn as_mut_slice(&mut self) -> &mut [u8];
    fn as_slice(&self) -> &[u8];
    fn paddr(&self) -> PhysAddr;
    fn vaddr(&self) -> VirtAddr;
}

pub trait QueuePage<const SIZE: usize>: DevicePage {
    fn as_descriptor_table_at<'a>(&self, offset: usize) -> &'a [Descriptor];
    fn as_mut_descriptor_table_at<'a>(&mut self, offset: usize) -> &'a mut [Descriptor];
    fn as_avail_ring_at<'a>(&self, offset: usize) -> &'a AvailRing<SIZE>;
    fn as_mut_avail_ring<'a>(&mut self, offset: usize) -> &'a mut AvailRing<SIZE>;
    fn as_used_ring<'a>(&self, offset: usize) -> &'a UsedRing<SIZE>;
    fn as_mut_used_ring<'a>(&mut self, offset: usize) -> &'a mut UsedRing<SIZE>;
}

pub trait Hal<const SIZE: usize>: Send + Sync {
    fn dma_alloc(pages: usize) -> Box<dyn QueuePage<SIZE>>;
    fn dma_alloc_buf(pages: usize) -> Box<dyn DevicePage>;
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
