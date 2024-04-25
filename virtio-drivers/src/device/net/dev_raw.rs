use core::arch::x86_64::__m256;
use core::mem::size_of;

use super::{EthernetAddress, Features, NetConfig, VirtioNetHdr};
use super::{MIN_BUFFER_LEN, NET_HDR_SIZE, QUEUE_RECEIVE, QUEUE_TRANSMIT, SUPPORTED_FEATURES};
use crate::error::{VirtIoError, VirtIoResult};
use crate::hal::Hal;
use crate::queue::{DescFlag, Descriptor, VirtIoQueue};
use crate::transport::Transport;
use crate::volatile::ReadVolatile;
use alloc::vec;
use log::{debug, info, warn};

/// Raw driver for a VirtIO block device.
///
/// This is a raw version of the VirtIONet driver. It provides non-blocking
/// methods for transmitting and receiving raw slices, without the buffer
/// management. For more higher-level fucntions such as receive buffer backing,
/// see [`VirtIONet`].
///
/// [`VirtIONet`]: super::VirtIONet
pub struct VirtIONetRaw<H: Hal<QUEUE_SIZE>, T: Transport, const QUEUE_SIZE: usize> {
    transport: T,
    mac: EthernetAddress,
    recv_queue: VirtIoQueue<H, QUEUE_SIZE>,
    send_queue: VirtIoQueue<H, QUEUE_SIZE>,
}

impl<H: Hal<QUEUE_SIZE>, T: Transport, const QUEUE_SIZE: usize> VirtIONetRaw<H, T, QUEUE_SIZE> {
    /// Create a new VirtIO-Net driver.
    pub fn new(mut transport: T) -> VirtIoResult<Self> {
        let negotiated_features = transport.begin_init(SUPPORTED_FEATURES);
        info!("negotiated_features {:?}", negotiated_features);
        // read configuration space
        let config = NetConfig::default();
        let io_region = transport.io_region();
        let mac = config.mac.read(io_region)?;
        // Safe because config points to a valid MMIO region for the config space.
        debug!(
            "Got MAC={:02x?}, status={:?}",
            mac,
            config.status.read(io_region)
        );
        let recv_queue = VirtIoQueue::new(&mut transport, QUEUE_RECEIVE)?;
        let send_queue = VirtIoQueue::new(&mut transport, QUEUE_TRANSMIT)?;

        transport.finish_init();

        Ok(VirtIONetRaw {
            transport,
            mac: mac.into(),
            recv_queue,
            send_queue,
        })
    }

    /// Acknowledge interrupt.
    pub fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        self.transport.ack_interrupt()
    }

    /// Disable interrupts.
    // pub fn disable_interrupts(&mut self) -> VirtIoResult<()> {
    //     self.send_queue.set_dev_notify(false)?;
    //     self.recv_queue.set_dev_notify(false)?;
    // }

    /// Enable interrupts.
    // pub fn enable_interrupts(&mut self) {
    //     self.send_queue.set_dev_notify(true);
    //     self.recv_queue.set_dev_notify(true);
    // }

    /// Get MAC address.
    pub fn mac_address(&self) -> VirtIoResult<[u8; 6]> {
        Ok(self.mac.into())
    }

    /// Whether can send packet.
    pub fn can_send(&self) -> bool {
        self.send_queue.available_desc() >= 2
    }

    /// Whether the length of the receive buffer is valid.
    fn check_rx_buf_len(rx_buf: &[u8]) -> VirtIoResult<()> {
        if rx_buf.len() < MIN_BUFFER_LEN {
            warn!("Receive buffer len {} is too small", rx_buf.len());
            Err(VirtIoError::InvalidParam)
        } else {
            Ok(())
        }
    }

    /// Whether the length of the transmit buffer is valid.
    fn check_tx_buf_len(tx_buf: &[u8]) -> VirtIoResult<()> {
        if tx_buf.len() < NET_HDR_SIZE {
            warn!("Transmit buffer len {} is too small", tx_buf.len());
            Err(VirtIoError::InvalidParam)
        } else {
            Ok(())
        }
    }

    /// Fill the header of the `buffer` with [`VirtioNetHdr`].
    ///
    /// If the `buffer` is not large enough, it returns [`Error::InvalidParam`].
    pub fn fill_buffer_header(&self, buffer: &mut [u8]) -> VirtIoResult<usize> {
        if buffer.len() < NET_HDR_SIZE {
            return Err(VirtIoError::InvalidParam);
        }
        VirtioNetHdr::default().write_to(&mut buffer[..NET_HDR_SIZE]);
        Ok(NET_HDR_SIZE)
    }

    /// Submits a request to transmit a buffer immediately without waiting for
    /// the transmission to complete.
    ///
    /// It will submit request to the VirtIO net device and return a token
    /// identifying the position of the first descriptor in the chain. If there
    /// are not enough descriptors to allocate, then it returns
    /// [`Error::QueueFull`].
    ///
    /// The caller needs to fill the `tx_buf` with a header by calling
    /// [`fill_buffer_header`] before transmission. Then it calls [`poll_transmit`]
    /// with the returned token to check whether the device has finished handling
    /// the request. Once it has, the caller must call [`transmit_complete`] with
    /// the same buffer before reading the result (transmitted length).
    ///
    /// # Safety
    ///
    /// `tx_buf` is still borrowed by the underlying VirtIO net device even after
    /// this method returns. Thus, it is the caller's responsibility to guarantee
    /// that they are not accessed before the request is completed in order to
    /// avoid data races.
    ///
    /// [`fill_buffer_header`]: Self::fill_buffer_header
    /// [`poll_transmit`]: Self::poll_transmit
    /// [`transmit_complete`]: Self::transmit_complete
    pub fn transmit_begin(&mut self, tx_buf: &[u8]) -> VirtIoResult<u16> {
        Self::check_tx_buf_len(tx_buf)?;
        let desc = Descriptor::new(tx_buf.as_ptr() as _, tx_buf.len() as _, DescFlag::EMPTY);
        let token = self.send_queue.add(vec![desc])?;
        if self.send_queue.should_notify() {
            self.transport.notify(QUEUE_TRANSMIT);
        }
        Ok(token)
    }

    /// Fetches the token of the next completed transmission request from the
    /// used ring and returns it, without removing it from the used ring. If
    /// there are no pending completed requests it returns [`None`].
    pub fn poll_transmit(&mut self, token: u16) -> VirtIoResult<bool> {
        self.send_queue.can_pop(token)
    }

    /// Completes a transmission operation which was started by [`transmit_begin`].
    /// Returns number of bytes transmitted.
    ///
    /// # Safety
    ///
    /// The same buffer must be passed in again as was passed to
    /// [`transmit_begin`] when it returned the token.
    ///
    /// [`transmit_begin`]: Self::transmit_begin
    pub fn transmit_complete(&mut self, token: u16) -> VirtIoResult<usize> {
        let len = self.send_queue.pop_used(token)?;
        Ok(len as usize)
    }

    /// Submits a request to receive a buffer immediately without waiting for
    /// the reception to complete.
    ///
    /// It will submit request to the VirtIO net device and return a token
    /// identifying the position of the first descriptor in the chain. If there
    /// are not enough descriptors to allocate, then it returns
    /// [`Error::QueueFull`].
    ///
    /// The caller can then call [`poll_receive`] with the returned token to
    /// check whether the device has finished handling the request. Once it has,
    /// the caller must call [`receive_complete`] with the same buffer before
    /// reading the response.
    ///
    /// # Safety
    ///
    /// `rx_buf` is still borrowed by the underlying VirtIO net device even after
    /// this method returns. Thus, it is the caller's responsibility to guarantee
    /// that they are not accessed before the request is completed in order to
    /// avoid data races.
    ///
    /// [`poll_receive`]: Self::poll_receive
    /// [`receive_complete`]: Self::receive_complete
    pub fn receive_begin(&mut self, rx_buf: &mut [u8]) -> VirtIoResult<u16> {
        Self::check_rx_buf_len(rx_buf)?;
        let desc = Descriptor::new(rx_buf.as_ptr() as _, rx_buf.len() as _, DescFlag::WRITE);
        let token = self.recv_queue.add(vec![desc])?;
        if self.recv_queue.should_notify() {
            self.transport.notify(QUEUE_RECEIVE);
        }
        Ok(token)
    }

    /// Fetches the token of the next completed reception request from the
    /// used ring and returns it, without removing it from the used ring. If
    /// there are no pending completed requests it returns [`None`].
    pub fn poll_receive(&self, token: u16) -> VirtIoResult<bool> {
        self.recv_queue.can_pop(token)
    }

    /// Completes a transmission operation which was started by [`receive_begin`].
    ///
    /// After completion, the `rx_buf` will contain a header followed by the
    /// received packet. It returns the length of the header and the length of
    /// the packet.
    ///
    /// # Safety
    ///
    /// The same buffer must be passed in again as was passed to
    /// [`receive_begin`] when it returned the token.
    ///
    /// [`receive_begin`]: Self::receive_begin
    pub fn receive_complete(&mut self, token: u16) -> VirtIoResult<(usize, usize)> {
        let len = self.recv_queue.pop_used(token)? as usize;
        let packet_len = len.checked_sub(NET_HDR_SIZE).ok_or(VirtIoError::IoError)?;
        Ok((NET_HDR_SIZE, packet_len))
    }

    /// Sends a packet to the network, and blocks until the request completed.
    pub fn send(&mut self, tx_buf: &[u8]) -> VirtIoResult<()> {
        let mut header_buf = [0u8; size_of::<VirtioNetHdr>()];
        VirtioNetHdr::default().write_to(&mut header_buf);

        let header_desc = Descriptor::new(
            header_buf.as_ptr() as _,
            header_buf.len() as _,
            if tx_buf.is_empty() {
                DescFlag::EMPTY
            } else {
                DescFlag::NEXT
            },
        );
        let v;
        if !tx_buf.is_empty() {
            // Special case sending an empty packet, to avoid adding an empty buffer to the
            // virtqueue.
            let desc = Descriptor::new(tx_buf.as_ptr() as _, tx_buf.len() as _, DescFlag::EMPTY);
            v = vec![header_desc, desc];
        } else {
            v = vec![header_desc];
        }
        self.send_queue
            .add_notify_wait_pop(&mut self.transport, v)?;
        Ok(())
    }

    /// Blocks and waits for a packet to be received.
    ///
    /// After completion, the `rx_buf` will contain a header followed by the
    /// received packet. It returns the length of the header and the length of
    /// the packet.
    pub fn receive_wait(&mut self, rx_buf: &mut [u8]) -> VirtIoResult<(usize, usize)> {
        let token = self.receive_begin(rx_buf)?;
        while !self.poll_receive(token)? {
            core::hint::spin_loop();
        }
        self.receive_complete(token)
    }
}
