//! Driver for VirtIO network devices.

mod raw;
mod ty;

extern crate alloc;
use crate::{
    error::{VirtIoError, VirtIoResult},
    hal::Hal,
    transport::Transport,
};
use alloc::vec::Vec;
pub use raw::VirtIONetRaw;

/// Driver for a VirtIO network device.
///
/// Unlike [`VirtIONetRaw`], it uses [`RxBuffer`]s for transmission and
/// reception rather than the raw slices. On initialization, it pre-allocates
/// all receive buffers and puts them all in the receive queue.
///
/// The virtio network device is a virtual ethernet card.
///
/// It has enhanced rapidly and demonstrates clearly how support for new
/// features are added to an existing device.
/// Empty buffers are placed in one virtqueue for receiving packets, and
/// outgoing packets are enqueued into another for transmission in that order.
/// A third command queue is used to control advanced filtering features.
pub struct VirtIONet<H: Hal<QUEUE_SIZE>, T: Transport, const QUEUE_SIZE: usize> {
    inner: VirtIONetRaw<H, T, QUEUE_SIZE>,
    rx_buffers: [Vec<u8>; QUEUE_SIZE],
}

impl<H: Hal<QUEUE_SIZE>, T: Transport, const QUEUE_SIZE: usize> VirtIONet<H, T, QUEUE_SIZE> {
    /// Create a new VirtIO-Net driver.
    pub fn new(transport: T, buf_len: usize) -> VirtIoResult<Self> {
        let mut inner = VirtIONetRaw::new(transport)?;

        const NONE_BUF: Vec<u8> = Vec::new();
        let mut rx_buffers = [NONE_BUF; QUEUE_SIZE];
        for (i, rx_buf) in rx_buffers.iter_mut().enumerate() {
            rx_buf.resize(buf_len, 0);
            // Safe because the buffer lives as long as the queue.
            let token = inner.receive_begin(rx_buf.as_mut())?;
            assert_eq!(token, i as u16);
        }

        Ok(VirtIONet { inner, rx_buffers })
    }

    /// Acknowledge interrupt.
    pub fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        self.inner.ack_interrupt()
    }

    /// Disable interrupts.
    // pub fn disable_interrupts(&mut self) -> VirtIoResult<()> {
    //     self.inner.disable_interrupts()
    // }

    /// Enable interrupts.
    // pub fn enable_interrupts(&mut self) -> VirtIoResult<()> {
    //     self.inner.disable_interrupts()
    // }

    /// Get MAC address.
    pub fn mac_address(&self) -> VirtIoResult<[u8; 6]> {
        self.inner.mac_address()
    }

    /// Whether can send packet.
    pub fn can_send(&self) -> VirtIoResult<bool> {
        self.inner.can_send()
    }

    /// Whether can receive packet. If can, return (token, packet length).
    pub fn can_recv(&self) -> VirtIoResult<Option<(u16, usize)>> {
        self.inner.can_recv()
    }

    /// Receives a `[u8]` from network and return length. If currently no data, returns an
    /// error with type [`Error::NotReady`].
    ///
    /// It will try to pop a buffer that completed data reception in the
    /// NIC queue.
    pub fn receive(&mut self, data: &mut [u8]) -> VirtIoResult<usize> {
        if let Some((token, _)) = self.inner.can_recv()? {
            let rx_buf = &self.rx_buffers[token as usize];

            let (hdr_len, pkt_len) = self.inner.receive_complete(token)?;
            (data[0..pkt_len]).copy_from_slice(&rx_buf[hdr_len..(hdr_len + pkt_len)]);
            Ok(pkt_len)
        } else {
            Err(VirtIoError::NotReady)
        }
    }

    /// Sends a [`TxBuffer`] to the network, and blocks until the request
    /// completed.
    pub fn send(&mut self, tx_buf: &[u8]) -> VirtIoResult<()> {
        self.inner.send(tx_buf)
    }
}
