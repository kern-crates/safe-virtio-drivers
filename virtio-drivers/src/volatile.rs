use core::marker::PhantomData;

use crate::common::Array;
use crate::error::VirtIoResult;
use crate::hal::VirtIoDeviceIo;

#[derive(Debug, Default)]
pub struct ReadOnly<const OFFSET: usize, T: Copy> {
    _marker: PhantomData<T>,
}
#[derive(Debug, Default)]
pub struct WriteOnly<const OFFSET: usize, T: Copy> {
    _marker: PhantomData<T>,
}
#[derive(Debug, Default)]
pub struct ReadWrite<const OFFSET: usize, T: Copy> {
    _marker: PhantomData<T>,
}

pub trait ReadVolatile {
    type T;
    fn read(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<Self::T>;
}

pub trait WriteVolatile {
    type T;
    fn write(&self, data: Self::T, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()>;
}

// TODO: use macro to simpify code
impl<const OFFSET: usize, const SIZE: usize> ReadVolatile for ReadOnly<OFFSET, Array<SIZE, u8>> {
    type T = [u8; SIZE];
    #[inline]
    fn read(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<Self::T> {
        let mut res = [0; SIZE];
        for i in 0..SIZE {
            res[i] = io_region.read_volatile_u8_at(OFFSET + i)?;
        }
        Ok(res)
    }
}
impl<const OFFSET: usize> ReadVolatile for ReadOnly<OFFSET, u32> {
    type T = u32;
    #[inline]
    fn read(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<Self::T> {
        io_region.read_volatile_u32_at(OFFSET)
    }
}
impl<const OFFSET: usize> ReadVolatile for ReadOnly<OFFSET, u16> {
    type T = u16;
    #[inline]
    fn read(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<Self::T> {
        io_region.read_volatile_u32_at(OFFSET).map(|x| x as Self::T)
    }
}
impl<const OFFSET: usize> ReadVolatile for ReadOnly<OFFSET, u8> {
    type T = u8;
    #[inline]
    fn read(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<Self::T> {
        io_region.read_volatile_u8_at(OFFSET)
    }
}
impl<const OFFSET: usize> WriteVolatile for WriteOnly<OFFSET, u64> {
    type T = u64;
    #[inline]
    fn write(&self, data: u64, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, data as u32)?;
        io_region.write_volatile_u32_at(OFFSET + 0x4, (data >> 32) as u32)
    }
}
impl<const OFFSET: usize> WriteVolatile for WriteOnly<OFFSET, u32> {
    type T = u32;
    #[inline]
    fn write(&self, data: u32, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, data)
    }
}
impl<const OFFSET: usize> WriteVolatile for WriteOnly<OFFSET, u8> {
    type T = u8;
    #[inline]
    fn write(&self, data: u8, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()> {
        io_region.write_volatile_u8_at(OFFSET, data)
    }
}
impl<const OFFSET: usize> ReadVolatile for ReadWrite<OFFSET, u32> {
    type T = u32;
    #[inline]
    fn read(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<Self::T> {
        io_region.read_volatile_u32_at(OFFSET)
    }
}
impl<const OFFSET: usize> ReadVolatile for ReadWrite<OFFSET, u8> {
    type T = u8;
    #[inline]
    fn read(&self, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<Self::T> {
        io_region.read_volatile_u8_at(OFFSET)
    }
}
impl<const OFFSET: usize> WriteVolatile for ReadWrite<OFFSET, u32> {
    type T = u32;
    #[inline]
    fn write(&self, data: u32, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()> {
        io_region.write_volatile_u32_at(OFFSET, data)
    }
}
impl<const OFFSET: usize> WriteVolatile for ReadWrite<OFFSET, u8> {
    type T = u8;
    #[inline]
    fn write(&self, data: u8, io_region: &dyn VirtIoDeviceIo) -> VirtIoResult<()> {
        io_region.write_volatile_u8_at(OFFSET, data)
    }
}
