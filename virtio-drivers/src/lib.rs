#![no_std]
#![forbid(unsafe_code)]
#![allow(unused)]
extern crate alloc;
pub mod device;
pub mod hal;
mod volatile;
pub mod error;
mod transport;
pub mod queue;


pub const PAGE_SIZE: usize = 4096;

pub type VirtAddr = usize;
pub type PhysAddr = usize;



/// The number of pages required to store `size` bytes, rounded up to a whole number of pages.
const fn pages(size: usize) -> usize {
    (size + PAGE_SIZE - 1) / PAGE_SIZE
}
