use crate::error::VirtIoResult;
use crate::hal::VirtIoDeviceIo;
use alloc::boxed::Box;

#[derive(Debug, Default)]
pub struct ReadOnly<const OFFSET: usize>;
#[derive(Debug, Default)]
pub struct WriteOnly<const OFFSET: usize>;
#[derive(Debug, Default)]
pub struct ReadWrite<const OFFSET: usize>;

pub trait ReadVolatile {
    fn read_volatile_u32(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<u32>;
    fn read_volatile_u64(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<u64>;
}

pub trait WriteVolatile {
    fn write_volatile_u32(
        &self,
        data: u32,
        io_region: &Box<dyn VirtIoDeviceIo>,
    ) -> VirtIoResult<()>;
    fn write_volatile_u64(
        &self,
        data: u64,
        io_region: &Box<dyn VirtIoDeviceIo>,
    ) -> VirtIoResult<()>;
}

impl<const OFFSET: usize> ReadVolatile for ReadOnly<OFFSET> {
    #[inline]
    fn read_volatile_u32(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<u32> {
        io_region.read_volatile_u32_at(OFFSET)
    }
    #[inline]
    fn read_volatile_u64(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<u64> {
        let low = io_region.read_volatile_u32_at(OFFSET)?;
        let high = io_region.read_volatile_u32_at(OFFSET + 4)?;
        Ok((high as u64) << 32 | low as u64)
    }
}

impl<const OFFSET: usize> WriteVolatile for WriteOnly<OFFSET> {
    #[inline]
    fn write_volatile_u32(
        &self,
        data: u32,
        io_region: &Box<dyn VirtIoDeviceIo>,
    ) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, data)
    }
    #[inline]
    fn write_volatile_u64(
        &self,
        data: u64,
        io_region: &Box<dyn VirtIoDeviceIo>,
    ) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, (data & 0xffff_ffff) as u32)?;
        io_region.write_volatile_u32_at(OFFSET + 4, (data >> 32) as u32)
    }
}

impl<const OFFSET: usize> ReadVolatile for ReadWrite<OFFSET> {
    fn read_volatile_u32(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<u32> {
        io_region.read_volatile_u32_at(OFFSET)
    }
    fn read_volatile_u64(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<u64> {
        ReadOnly::<OFFSET>.read_volatile_u64(io_region)
    }
}

impl<const OFFSET: usize> WriteVolatile for ReadWrite<OFFSET> {
    fn write_volatile_u32(
        &self,
        data: u32,
        io_region: &Box<dyn VirtIoDeviceIo>,
    ) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, data)
    }
    fn write_volatile_u64(
        &self,
        data: u64,
        io_region: &Box<dyn VirtIoDeviceIo>,
    ) -> VirtIoResult<()> {
        WriteOnly::<OFFSET>.write_volatile_u64(data, io_region)
    }
}
