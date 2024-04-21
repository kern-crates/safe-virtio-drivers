use crate::error::VirtIoResult;
use crate::hal::Hal;
use crate::queue::{DescFlag, Descriptor, VirtIoQueue};
use crate::transport::Transport;
use alloc::boxed::Box;

const QUEUE_EVENT: u16 = 0;
const QUEUE_STATUS: u16 = 1;
const SUPPORTED_FEATURES: Feature = Feature::RING_EVENT_IDX;
// a parameter that can change
const QUEUE_SIZE: usize = 32;

mod ty;
use ty::*;

/// Virtual human interface devices such as keyboards, mice and tablets.
///
/// An instance of the virtio device represents one such input device.
/// Device behavior mirrors that of the evdev layer in Linux,
/// making pass-through implementations on top of evdev easy.
pub struct VirtIOInput<H: Hal<QUEUE_SIZE>, T: Transport> {
    transport: T,
    event_queue: VirtIoQueue<H, QUEUE_SIZE>,
    status_queue: VirtIoQueue<H, QUEUE_SIZE>,
    event_buf: Box<[InputEvent; 32]>,
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> VirtIOInput<H, T> {
    /// Create a new VirtIO-Input driver.
    pub fn new(mut transport: T) -> VirtIoResult<Self> {
        let mut event_buf = Box::new([InputEvent::default(); QUEUE_SIZE]);

        let negotiated_features = transport.begin_init(SUPPORTED_FEATURES);

        let config = InputConfig::default();

        let mut event_queue = VirtIoQueue::new(&mut transport, QUEUE_EVENT)?;
        let status_queue = VirtIoQueue::new(&mut transport, QUEUE_STATUS)?;
        for (i, event) in event_buf.as_mut().iter_mut().enumerate() {
            // Safe because the buffer lasts as long as the queue.
            let token = unsafe { event_queue.add(&[], &mut [event.as_bytes_mut()])? };
            assert_eq!(token, i as u16);
        }
        if event_queue.should_notify() {
            transport.notify(QUEUE_EVENT);
        }

        transport.finish_init();

        Ok(VirtIOInput {
            transport,
            event_queue,
            status_queue,
            event_buf,
        })
    }

    /// Acknowledge interrupt and process events.
    pub fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        self.transport.ack_interrupt()
    }

    /// Pop the pending event.
    pub fn pop_pending_event(&mut self) -> Option<InputEvent> {
        if let Some(token) = self.event_queue.peek_used() {
            let event = &mut self.event_buf[token as usize];
            // Safe because we are passing the same buffer as we passed to `VirtQueue::add` and it
            // is still valid.
            unsafe {
                self.event_queue
                    .pop_used(token, &[], &mut [event.as_bytes_mut()])
                    .ok()?;
            }
            let event_saved = *event;
            // requeue
            // Safe because buffer lasts as long as the queue.
            if let Ok(new_token) = unsafe { self.event_queue.add(&[], &mut [event.as_bytes_mut()]) }
            {
                // This only works because nothing happen between `pop_used` and `add` that affects
                // the list of free descriptors in the queue, so `add` reuses the descriptor which
                // was just freed by `pop_used`.
                assert_eq!(new_token, token);
                if self.event_queue.should_notify() {
                    self.transport.notify(QUEUE_EVENT);
                }
                return Some(event_saved);
            }
        }
        None
    }

    /// Query a specific piece of information by `select` and `subsel`, and write
    /// result to `out`, return the result size.
    pub fn query_config_select(
        &mut self,
        select: InputConfigSelect,
        subsel: u8,
        out: &mut [u8],
    ) -> u8 {
        let size;
        let data;
        // Safe because config points to a valid MMIO region for the config space.

        unsafe {
            volwrite!(self.config, select, select as u8);
            volwrite!(self.config, subsel, subsel);
            size = volread!(self.config, size);
            data = volread!(self.config, data);
        }
        out[..size as usize].copy_from_slice(&data[..size as usize]);
        size
    }
}

// impl<H: Hal<>, T: Transport> Drop for VirtIOInput<H, T> {
//     fn drop(&mut self) {
//         // Clear any pointers pointing to DMA regions, so the device doesn't try to access them
//         // after they have been freed.
//         self.transport.queue_unset(QUEUE_EVENT);
//         self.transport.queue_unset(QUEUE_STATUS);
//     }
// }
