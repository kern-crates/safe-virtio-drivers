#![feature(riscv_ext_intrinsics)]
#![no_std]
#![no_main]
// #![deny(warnings)]

#[macro_use]
extern crate log;

extern crate alloc;
extern crate opensbi_rt;

use crate::my_impl::{MyHalImpl, SafeIoRegion};
use alloc::boxed::Box;
use alloc::vec;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use core::sync::atomic::AtomicUsize;
use fdt::{node::FdtNode, standard_nodes::Compatible, Fdt};
use log::LevelFilter;
use opensbi_rt::{print, println};
use spin::Lazy;
use virtio_drivers::device::console::VirtIOConsole;
use virtio_drivers::{
    device::{blk::VirtIOBlk, gpu::VirtIOGpu, input::VirtIOInput},
    transport::{
        mmio::{MmioTransport, VirtIOHeader},
        DeviceType, Transport,
    },
};
use virtio_impl::HalImpl;

mod virtio_impl;

mod my_impl;
#[cfg(feature = "tcp")]
mod tcp;

extern "C" {
    fn end();
}

static DMA_PADDR: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(end as usize));

const NET_QUEUE_SIZE: usize = 16;

#[no_mangle]
extern "C" fn main(_hartid: usize, device_tree_paddr: usize) {
    log::set_max_level(LevelFilter::Info);
    init_dt(device_tree_paddr);
    info!("test end");
}

fn init_dt(dtb: usize) {
    info!("device tree @ {:#x}", dtb);
    // Safe because the pointer is a valid pointer to unaliased memory.
    let fdt = unsafe { Fdt::from_ptr(dtb as *const u8).unwrap() };
    walk_dt(fdt);
}

fn walk_dt(fdt: Fdt) {
    for node in fdt.all_nodes() {
        if let Some(compatible) = node.compatible() {
            if compatible.all().any(|s| s == "virtio,mmio") {
                virtio_probe(node);
            }
        }
    }
}

fn virtio_probe(node: FdtNode) {
    if let Some(reg) = node.reg().and_then(|mut reg| reg.next()) {
        let paddr = reg.starting_address as usize;
        let size = reg.size.unwrap();
        let vaddr = paddr;
        info!("walk dt addr={:#x}, size={:#x}", paddr, size);
        info!(
            "Device tree node {}: {:?}",
            node.name,
            node.compatible().map(Compatible::first),
        );
        let header = NonNull::new(vaddr as *mut VirtIOHeader).unwrap();
        match unsafe { MmioTransport::new(header) } {
            Err(e) => warn!("Error creating VirtIO MMIO transport: {}", e),
            Ok(transport) => {
                warn!(
                    "Detected virtio MMIO device with vendor id {:#X}, device type {:?}, version {:?}",
                    transport.vendor_id(),
                    transport.device_type(),
                    transport.version(),
                );
                virtio_device(transport, vaddr, size);
            }
        }
    }
}

fn virtio_device(transport: impl Transport, vaddr: usize, size: usize) {
    match transport.device_type() {
        DeviceType::Block => virtio_blk(vaddr, size),
        DeviceType::Input => virtio_input(vaddr, size),
        DeviceType::Console => virtio_console(vaddr, size),
        DeviceType::GPU => virtio_gpu(vaddr, size),
        DeviceType::Network => virtio_net(vaddr, size),
        t => warn!("Unrecognized virtio device: {:?}", t),
    }
}
type MyTransport = safe_virtio_drivers::transport::mmio::MmioTransport;
fn virtio_blk(vaddr: usize, size: usize) {
    // let mut blk = VirtIOBlk::<HalImpl, T>::new(transport).expect("failed to create blk driver");
    let io_region = SafeIoRegion::new(vaddr, size);
    //

    let transport = MyTransport::new(Box::new(io_region)).expect("failed to create transport");

    let mut blk =
        safe_virtio_drivers::device::block::VirtIOBlk::<MyHalImpl, MyTransport>::new(transport)
            .expect("failed to create blk driver");

    let mut input = vec![0xffu8; 512];
    let mut output = vec![0; 512];
    let iter = 10 * 1024 * 1024 / 512;
    for i in 0..iter {
        for x in input.iter_mut() {
            *x = i as u8;
        }
        blk.write_blocks(i, &input).expect("failed to write");
        blk.read_blocks(i, &mut output).expect("failed to read");
        assert_eq!(input, output);
    }
    blk.flush().expect("failed to flush");
    info!("virtio-blk test finished");
}

fn virtio_gpu(vaddr: usize, size: usize) {
    // let mut gpu = VirtIOGpu::<HalImpl, T>::new(transport).expect("failed to create gpu driver");
    let io_region = SafeIoRegion::new(vaddr, size);
    let transport = MyTransport::new(Box::new(io_region)).expect("failed to create transport");
    let mut gpu =
        safe_virtio_drivers::device::gpu::VirtIOGpu::<MyHalImpl, MyTransport>::new(transport)
            .expect("failed to create gpu driver");
    let (width, height) = gpu.resolution().expect("failed to get resolution");
    let width = width as usize;
    let height = height as usize;
    info!("GPU resolution is {}x{}", width, height);
    let fb = gpu.setup_framebuffer().expect("failed to get fb");
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            fb[idx] = x as u8;
            fb[idx + 1] = y as u8;
            fb[idx + 2] = (x + y) as u8;
        }
    }
    gpu.flush().expect("failed to flush");
    //delay some time
    info!("virtio-gpu show graphics....");
    for _ in 0..10000 {
        for _ in 0..100000 {
            unsafe {
                core::arch::asm!("nop");
            }
        }
    }
    info!("virtio-gpu test finished");
}

fn virtio_input(va: usize, size: usize) {
    //let mut event_buf = [0u64; 32];
    // let mut _input =
    //     VirtIOInput::<HalImpl, T>::new(transport).expect("failed to create input driver");
    let ior = SafeIoRegion::new(va, size);
    let tr = MyTransport::new(Box::new(ior)).expect("transport create failed");
    let mut input =
        safe_virtio_drivers::device::input::VirtIOInput::<MyHalImpl, MyTransport>::new(tr)
            .expect("input driver create failed");

    info!("testing input... Press ESC or right-click to continue.");
    loop {
        input.ack_interrupt().expect("fail to ack");
        if let Some(e) = input.pop_pending_event().expect("pop failed") {
            info!("input: {:?}", e);
            if e.event_type == 1 && (e.code == 1 || e.code == 273) && e.value == 0 {
                break;
            }
        }
    }
    // TODO: handle external interrupt
}

fn virtio_console(vaddr: usize, size: usize) {
    // let mut console = VirtIOConsole::<HalImpl, _>::new(transport).unwrap();
    let io_region = SafeIoRegion::new(vaddr, size);
    let transport = MyTransport::new(Box::new(io_region)).expect("failed to create transport");
    let mut console =
        safe_virtio_drivers::device::console::VirtIOConsole::<MyHalImpl, MyTransport>::new(
            transport,
        )
        .expect("failed to create console driver");

    let info = console.info().unwrap();
    println!("VirtIO console {} x {}", info.rows, info.columns);

    for &c in b"Hello console!\n" {
        console.send(c).expect("failed to send to console");
    }
    let c = console.recv(true).unwrap();
    println!("Read {:?} from console.", c)
}

fn virtio_net(vaddr: usize, size: usize) {
    let io_region = SafeIoRegion::new(vaddr, size);
    let transport = MyTransport::new(Box::new(io_region)).expect("failed to create transport");
    #[cfg(not(feature = "tcp"))]
    {
        let mut net = safe_virtio_drivers::device::net::VirtIONetRaw::<
            MyHalImpl,
            MyTransport,
            NET_QUEUE_SIZE,
        >::new(transport)
        .expect("failed to create net driver");
        info!("MAC address: {:02x?}", net.mac_address());

        let mut buf = [0u8; 2048];
        let (hdr_len, pkt_len) = net.receive_wait(&mut buf).expect("failed to recv");
        info!(
            "recv {} bytes: {:02x?}",
            pkt_len,
            &buf[hdr_len..hdr_len + pkt_len]
        );
        net.send(&buf[..hdr_len + pkt_len]).expect("failed to send");
        info!("virtio-net test finished");
    }

    #[cfg(feature = "tcp")]
    {
        const NET_BUFFER_LEN: usize = 2048;
        let net = safe_virtio_drivers::device::net::VirtIONet::<
            MyHalImpl,
            MyTransport,
            NET_QUEUE_SIZE,
        >::new(transport, NET_BUFFER_LEN)
        .expect("failed to create net driver");
        info!("MAC address: {:02x?}", net.mac_address());
        tcp::test_echo_server(net);
    }
}
