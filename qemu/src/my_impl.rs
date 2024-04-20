use crate::DMA_PADDR;
use alloc::boxed::Box;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use safe_virtio_drivers::error::VirtIoResult;
use safe_virtio_drivers::hal::{QueuePage, VirtIoDeviceIo};
use safe_virtio_drivers::queue::{AvailRing, Descriptor, UsedRing};
use safe_virtio_drivers::PAGE_SIZE;

pub struct MyHalImpl;

pub struct Page {
    pa: usize,
}

#[derive(Debug)]
pub struct SafeIoRegion {
    base: usize,
    len: usize,
}

impl SafeIoRegion {
    pub fn new(base: usize, len: usize) -> Self {
        SafeIoRegion { base, len }
    }
}

impl VirtIoDeviceIo for SafeIoRegion {
    #[inline]
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32> {
        let ptr = (self.base + off) as *const u32;
        Ok(unsafe { ptr.read_volatile() })
    }
    #[inline]
    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()> {
        let ptr = (self.base + off) as *mut u32;
        unsafe {
            ptr.write_volatile(data);
        }
        Ok(())
    }
}

impl<const SIZE: usize> safe_virtio_drivers::hal::Hal<SIZE> for MyHalImpl {
    fn dma_alloc(pages: usize) -> Box<dyn QueuePage<SIZE>> {
        let paddr = DMA_PADDR.fetch_add(PAGE_SIZE * pages, Ordering::SeqCst);
        info!("alloc DMA: paddr={:#x}, pages={}", paddr, pages);
        Box::new(Page { pa: paddr })
    }
}

impl<const SIZE: usize> QueuePage<SIZE> for Page {
    fn as_descriptor_table_at<'a>(&self, offset: usize) -> &'a [Descriptor] {
        unsafe {
            let ptr = (self.pa + offset) as *const Descriptor;
            core::slice::from_raw_parts(ptr, SIZE)
        }
    }

    fn as_mut_descriptor_table_at<'a>(&mut self, offset: usize) -> &'a mut [Descriptor] {
        unsafe {
            let ptr = (self.pa + offset) as *mut Descriptor;
            core::slice::from_raw_parts_mut(ptr, SIZE)
        }
    }

    fn as_avail_ring_at<'a>(&self, offset: usize) -> &'a AvailRing<SIZE> {
        unsafe {
            let ptr = (self.pa + offset) as *const AvailRing<SIZE>;
            &*ptr
        }
    }

    fn as_mut_avail_ring<'a>(&mut self, offset: usize) -> &'a mut AvailRing<SIZE> {
        unsafe {
            let ptr = (self.pa + offset) as *mut AvailRing<SIZE>;
            &mut *ptr
        }
    }

    fn as_used_ring<'a>(&self, offset: usize) -> &'a UsedRing<SIZE> {
        unsafe {
            let ptr = (self.pa + offset) as *const UsedRing<SIZE>;
            &*ptr
        }
    }

    fn as_mut_used_ring<'a>(&mut self, offset: usize) -> &'a mut UsedRing<SIZE> {
        unsafe {
            let ptr = (self.pa + offset) as *mut UsedRing<SIZE>;
            &mut *ptr
        }
    }
}
