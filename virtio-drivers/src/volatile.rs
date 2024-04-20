use crate::error::VirtIoResult;
use crate::hal::VirtIoDeviceIo;

#[derive(Debug, Default)]
pub struct ReadOnly<const OFFSET: usize>;
#[derive(Debug, Default)]
pub struct WriteOnly<const OFFSET: usize>;
#[derive(Debug, Default)]
pub struct ReadWrite<const OFFSET: usize>;

pub trait ReadVolatile {
    fn read_u32(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<u32>;
}

pub trait WriteVolatile {
    fn write_u32(&self, data: u32, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()>;
}

impl<const OFFSET: usize> ReadVolatile for ReadOnly<OFFSET> {
    #[inline]
    fn read_u32(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<u32> {
        io_region.read_volatile_u32_at(OFFSET)
    }
}

impl<const OFFSET: usize> WriteVolatile for WriteOnly<OFFSET> {
    #[inline]
    fn write_u32(&self, data: u32, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, data)
    }
}

impl<const OFFSET: usize> ReadVolatile for ReadWrite<OFFSET> {
    #[inline]
    fn read_u32(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<u32> {
        io_region.read_volatile_u32_at(OFFSET)
    }
}

impl<const OFFSET: usize> WriteVolatile for ReadWrite<OFFSET> {
    #[inline]
    fn write_u32(&self, data: u32, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, data)
    }
}
