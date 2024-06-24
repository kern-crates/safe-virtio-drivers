# safe-virtio-drivers

This crate is ported from the [virtio-drivers](https://github.com/rcore-os/virtio-drivers) but with no unsafe code.


## The interface
```rust
pub trait VirtIoDeviceIo: Send + Sync + Debug {
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32>;
    fn read_volatile_u8_at(&self, off: usize) -> VirtIoResult<u8>;
    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()>;
    fn write_volatile_u8_at(&self, off: usize, data: u8) -> VirtIoResult<()>;
    fn paddr(&self) -> PhysAddr;
    fn vaddr(&self) -> VirtAddr;
}

pub trait DevicePage: Send + Sync {
    fn as_mut_slice(&mut self) -> &mut [u8];
    fn as_slice(&self) -> &[u8];
    fn paddr(&self) -> PhysAddr;
    fn vaddr(&self) -> VirtAddr;
}

pub trait QueuePage<const SIZE: usize>: DevicePage {
    fn queue_ref_mut(&mut self, layout: &QueueLayout) -> QueueMutRef<SIZE>;
}

pub trait Hal<const SIZE: usize>: Send + Sync {
    fn dma_alloc(pages: usize) -> Box<dyn QueuePage<SIZE>>;
    fn dma_alloc_buf(pages: usize) -> Box<dyn DevicePage>;
    fn to_paddr(va: usize) -> usize;
}
```

## Example
see [example](./qemu/src/my_impl.rs)

## TODO
- [ ] Add more virtio devices
