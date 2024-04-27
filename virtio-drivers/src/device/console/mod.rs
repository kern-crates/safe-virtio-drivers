mod ty;

use crate::error::VirtIoResult;
use crate::hal::Hal;
use crate::queue::{DescFlag, Descriptor, VirtIoQueue};
use crate::transport::Transport;
use crate::volatile::ReadVolatile;
use crate::PAGE_SIZE;
use alloc::boxed::Box;
use alloc::vec;
use log::info;
use ty::*;

const QUEUE_RECEIVEQ_PORT_0: u16 = 0;
const QUEUE_TRANSMITQ_PORT_0: u16 = 1;
const QUEUE_SIZE: usize = 2;
const SUPPORTED_FEATURES: ConsoleFeatures = ConsoleFeatures::empty();

pub struct VirtIOConsole<H: Hal<QUEUE_SIZE>, T: Transport> {
    transport: T,
    config_space: ConsoleConfig,
    receiveq: VirtIoQueue<H, QUEUE_SIZE>,
    transmitq: VirtIoQueue<H, QUEUE_SIZE>,
    queue_buf_rx: Box<[u8; PAGE_SIZE]>,
    cursor: usize,
    pending_len: usize,
    /// The token of the outstanding receive request, if there is one.
    receive_token: Option<u16>,
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> VirtIOConsole<H, T> {
    /// Create a new VirtIO console driver.
    pub fn new(mut transport: T) -> VirtIoResult<Self> {
        let _negotiated_features = transport.begin_init(SUPPORTED_FEATURES)?;
        let config_space = ConsoleConfig::default();
        let receiveq = VirtIoQueue::new(&mut transport, QUEUE_RECEIVEQ_PORT_0)?;
        let transmitq = VirtIoQueue::new(&mut transport, QUEUE_TRANSMITQ_PORT_0)?;
        transport.finish_init()?;
        Ok(Self {
            transport,
            config_space,
            receiveq,
            transmitq,
            queue_buf_rx: Box::new([0; PAGE_SIZE]),
            cursor: 0,
            pending_len: 0,
            receive_token: None,
        })
    }

    /// Returns a struct with information about the console device, such as the number of rows and columns.
    pub fn info(&self) -> VirtIoResult<ConsoleInfo> {
        let io_region = self.transport.io_region();
        let columns = self.config_space.cols.read(io_region)?;
        let rows = self.config_space.rows.read(io_region)?;
        let max_ports = self.config_space.max_nr_ports.read(io_region)?;
        Ok(ConsoleInfo {
            rows,
            columns,
            max_ports,
        })
    }
    /// Makes a request to the device to receive data, if there is not already an outstanding
    /// receive request or some data already received and not yet returned.
    fn poll_retrieve(&mut self) -> VirtIoResult<()> {
        // if receive_token is None, it means there is no outstanding receive request.
        // if cursor == pending_len, it means all data has been received.
        if self.receive_token.is_none() && self.cursor == self.pending_len {
            info!("poll_retrieve");
            // Safe because the buffer lasts at least as long as the queue, and there are no other
            // outstanding requests using the buffer.
            let req = Descriptor::new::<QUEUE_SIZE, H>(
                self.queue_buf_rx.as_ptr() as _,
                self.queue_buf_rx.len() as _,
                DescFlag::WRITE,
            );
            // let token = self.receiveq.add(vec![req])?;
            let l = self
                .receiveq
                .add_notify_wait_pop(&mut self.transport, vec![req])?;
            // if self.receiveq.should_notify() {
            //     info!("notify QUEUE_RECEIVEQ_PORT_0");
            //     self.transport.notify(QUEUE_RECEIVEQ_PORT_0)?;
            // }
            info!("poll_retrieve: l: {:?}", l);
            self.receive_token = Some(0);
        }
        Ok(())
    }

    /// If there is an outstanding receive request and it has finished, completes it.
    ///
    /// Returns true if new data has been received.
    fn finish_receive(&mut self) -> VirtIoResult<bool> {
        let mut flag = false;
        if let Some(receive_token) = self.receive_token {
            let peek_used = self.receiveq.peek_used();
            info!(
                "finish_receive: receive_token: {:?}, peek_used: {:?}",
                receive_token, peek_used
            );
            if self.receive_token == self.receiveq.peek_used() {
                let len = self.receiveq.pop_used(receive_token)?;
                flag = true;
                assert_ne!(len, 0);
                self.cursor = 0;
                self.pending_len = len as usize;
                // Clear `receive_token` so that when the buffer is used up the next call to
                // `poll_retrieve` will add a new pending request.
                self.receive_token.take();
            }
        }
        Ok(flag)
    }

    /// Returns the next available character from the console, if any.
    ///
    /// If no data has been received this will not block but immediately return `Ok<None>`.
    pub fn recv(&mut self, pop: bool) -> VirtIoResult<Option<u8>> {
        self.finish_receive()?;
        if self.cursor == self.pending_len {
            return Ok(None);
        }
        let ch = self.queue_buf_rx[self.cursor];
        if pop {
            self.cursor += 1;
            self.poll_retrieve()?;
        }
        Ok(Some(ch))
    }

    pub fn recv_block(&mut self) -> VirtIoResult<u8> {
        loop {
            self.finish_receive()?;
            self.poll_retrieve()?;
            if self.cursor == self.pending_len {
                // info!("cursor == pending_len");
                continue;
            }
            let ch = self.queue_buf_rx[self.cursor];
            self.cursor += 1;
            return Ok(ch);
        }
    }

    /// Sends a character to the console.
    pub fn send(&mut self, chr: u8) -> VirtIoResult<()> {
        let buf: [u8; 1] = [chr];
        let desc =
            Descriptor::new::<QUEUE_SIZE, H>(buf.as_ptr() as _, buf.len() as _, DescFlag::EMPTY);
        self.transmitq
            .add_notify_wait_pop(&mut self.transport, vec![desc])?;
        info!("send char: {:?}", chr as char);
        Ok(())
    }

    /// Acknowledges a pending interrupt, if any, and completes the outstanding finished read
    /// request if there is one.
    ///
    /// Returns true if new data has been received.
    pub fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        if !self.transport.ack_interrupt()? {
            return Ok(false);
        }
        self.finish_receive()
    }
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> Drop for VirtIOConsole<H, T> {
    fn drop(&mut self) {
        // Clear any pointers pointing to DMA regions, so the device doesn't try to access them
        // after they have been freed.
        self.transport
            .queue_unset(QUEUE_RECEIVEQ_PORT_0)
            .expect("failed to unset receive queue");
        self.transport
            .queue_unset(QUEUE_TRANSMITQ_PORT_0)
            .expect("failed to unset transmit queue")
    }
}
