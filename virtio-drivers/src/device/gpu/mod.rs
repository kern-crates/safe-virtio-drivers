mod ty;
use crate::error::{VirtIoError, VirtIoResult};
use crate::hal::{DevicePage, Hal};
use crate::pages;
use crate::queue::{DescFlag, Descriptor, VirtIoQueue};
use crate::transport::Transport;
use crate::volatile::ReadVolatile;
use alloc::boxed::Box;
use alloc::vec;
use core::mem::size_of_val;
use log::info;
use ty::*;

const QUEUE_SIZE: usize = 2;
const SUPPORTED_FEATURES: Features = Features::RING_EVENT_IDX;

/// A virtio based graphics adapter.
///
/// It can operate in 2D mode and in 3D (virgl) mode.
/// 3D mode will offload rendering ops to the host gpu and therefore requires
/// a gpu with 3D support on the host machine.
/// In 2D mode the virtio-gpu device provides support for ARGB Hardware cursors
/// and multiple scanouts (aka heads).
pub struct VirtIOGpu<H: Hal<QUEUE_SIZE>, T: Transport> {
    transport: T,
    rect: Option<Rect>,
    /// DMA area of frame buffer.
    frame_buffer_dma: Option<Box<dyn DevicePage>>,
    /// DMA area of cursor image buffer.
    cursor_buffer_dma: Option<Box<dyn DevicePage>>,
    /// Queue for sending control commands.
    control_queue: VirtIoQueue<H, QUEUE_SIZE>,
    /// Queue for sending cursor commands.
    cursor_queue: VirtIoQueue<H, QUEUE_SIZE>,
    config: GpuConfig,
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> VirtIOGpu<H, T> {
    /// Create a new VirtIO-GPU driver.
    pub fn new(mut transport: T) -> VirtIoResult<Self> {
        let _negotiated_features = transport.begin_init(SUPPORTED_FEATURES)?;
        let io_region = transport.io_region();
        // read config
        let config = GpuConfig::default();
        let events_read = config.events_read.read(io_region)?;
        let num_scanouts = config.num_scanouts.read(io_region)?;
        info!(
            "events_read: {:#x}, num_scanouts: {:#x}",
            events_read, num_scanouts
        );
        let control_queue = VirtIoQueue::new(&mut transport, 0)?;
        let cursor_queue = VirtIoQueue::new(&mut transport, 1)?;
        transport.finish_init()?;

        Ok(Self {
            transport,
            rect: None,
            frame_buffer_dma: None,
            cursor_buffer_dma: None,
            control_queue,
            cursor_queue,
            config,
        })
    }
    /// Acknowledge interrupt.
    pub fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        self.transport.ack_interrupt()
    }

    /// Get the resolution (width, height).
    pub fn resolution(&mut self) -> VirtIoResult<(u32, u32)> {
        let display_info = self.get_display_info()?;
        Ok((display_info.rect.width, display_info.rect.height))
    }

    /// Setup framebuffer
    pub fn setup_framebuffer(&mut self) -> VirtIoResult<&mut [u8]> {
        // get display info
        let display_info = self.get_display_info()?;
        info!("=> {:?}", display_info);
        self.rect = Some(display_info.rect);

        // create resource 2d
        self.resource_create_2d(
            RESOURCE_ID_FB,
            display_info.rect.width,
            display_info.rect.height,
        )?;

        // alloc continuous pages for the frame buffer
        let size = display_info.rect.width * display_info.rect.height * 4;
        let frame_buffer_dma = H::dma_alloc_buf(pages(size as usize));

        // resource_attach_backing
        self.resource_attach_backing(RESOURCE_ID_FB, frame_buffer_dma.paddr() as u64, size)?;

        // map frame buffer to screen
        self.set_scanout(display_info.rect, SCANOUT_ID, RESOURCE_ID_FB)?;
        self.frame_buffer_dma = Some(frame_buffer_dma);
        let buf = self.frame_buffer_dma.as_mut().unwrap().as_mut_slice();
        Ok(buf)
    }

    /// Flush framebuffer to screen.
    pub fn flush(&mut self) -> VirtIoResult<()> {
        let rect = self.rect.ok_or(VirtIoError::NotReady)?;
        // copy data from guest to host
        self.transfer_to_host_2d(rect, 0, RESOURCE_ID_FB)?;
        // flush data to screen
        self.resource_flush(rect, RESOURCE_ID_FB)?;
        Ok(())
    }

    /// Set the pointer shape and position.
    pub fn setup_cursor(
        &mut self,
        cursor_image: &[u8],
        pos_x: u32,
        pos_y: u32,
        hot_x: u32,
        hot_y: u32,
    ) -> VirtIoResult<()> {
        let size = CURSOR_RECT.width * CURSOR_RECT.height * 4;
        if cursor_image.len() != size as usize {
            return Err(VirtIoError::InvalidParam);
        }
        let mut cursor_buffer_dma = H::dma_alloc_buf(pages(size as usize));
        let buf = cursor_buffer_dma.as_mut_slice();
        buf.copy_from_slice(cursor_image);

        self.resource_create_2d(RESOURCE_ID_CURSOR, CURSOR_RECT.width, CURSOR_RECT.height)?;
        self.resource_attach_backing(RESOURCE_ID_CURSOR, cursor_buffer_dma.paddr() as u64, size)?;
        self.transfer_to_host_2d(CURSOR_RECT, 0, RESOURCE_ID_CURSOR)?;
        self.update_cursor(
            RESOURCE_ID_CURSOR,
            SCANOUT_ID,
            pos_x,
            pos_y,
            hot_x,
            hot_y,
            false,
        )?;
        self.cursor_buffer_dma = Some(cursor_buffer_dma);
        Ok(())
    }

    /// Move the pointer without updating the shape.
    pub fn move_cursor(&mut self, pos_x: u32, pos_y: u32) -> VirtIoResult<()> {
        self.update_cursor(RESOURCE_ID_CURSOR, SCANOUT_ID, pos_x, pos_y, 0, 0, true)?;
        Ok(())
    }

    /// Send a request to the device and block for a response.
    fn request<Req: Sized, Rsp: Sized>(&mut self, req: Req, rsp: Rsp) -> VirtIoResult<Rsp> {
        // self.queue_buf_send.copy_from_slice(req.as_slice());
        let req = Descriptor::new(
            &req as *const _ as _,
            size_of_val(&req) as _,
            DescFlag::NEXT,
        );
        let res = Descriptor::new(
            &rsp as *const _ as _,
            size_of_val(&rsp) as _,
            DescFlag::WRITE,
        );
        self.control_queue
            .add_notify_wait_pop(&mut self.transport, vec![req, res])?;
        Ok(rsp)
    }

    /// Send a mouse cursor operation request to the device and block for a response.
    fn cursor_request<Req: Sized>(&mut self, req: Req) -> VirtIoResult<()> {
        let req = Descriptor::new(
            &req as *const _ as _,
            size_of_val(&req) as _,
            DescFlag::NEXT,
        );
        self.cursor_queue
            .add_notify_wait_pop(&mut self.transport, vec![req])?;
        Ok(())
    }

    fn resource_create_2d(
        &mut self,
        resource_id: u32,
        width: u32,
        height: u32,
    ) -> VirtIoResult<()> {
        let req = ResourceCreate2D {
            header: CtrlHeader::with_type(Command::RESOURCE_CREATE_2D),
            resource_id,
            format: Format::B8G8R8A8UNORM,
            width,
            height,
        };
        let rsp: CtrlHeader = self.request(req, CtrlHeader::default())?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn set_scanout(&mut self, rect: Rect, scanout_id: u32, resource_id: u32) -> VirtIoResult<()> {
        let req = SetScanout {
            header: CtrlHeader::with_type(Command::SET_SCANOUT),
            rect,
            scanout_id,
            resource_id,
        };
        let rsp = self.request(req, CtrlHeader::default())?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn resource_flush(&mut self, rect: Rect, resource_id: u32) -> VirtIoResult<()> {
        let req = ResourceFlush {
            header: CtrlHeader::with_type(Command::RESOURCE_FLUSH),
            rect,
            resource_id,
            _padding: 0,
        };
        let rsp = self.request(req, CtrlHeader::default())?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn transfer_to_host_2d(
        &mut self,
        rect: Rect,
        offset: u64,
        resource_id: u32,
    ) -> VirtIoResult<()> {
        let req = TransferToHost2D {
            header: CtrlHeader::with_type(Command::TRANSFER_TO_HOST_2D),
            rect,
            offset,
            resource_id,
            _padding: 0,
        };
        let rsp = self.request(req, CtrlHeader::default())?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn resource_attach_backing(
        &mut self,
        resource_id: u32,
        paddr: u64,
        length: u32,
    ) -> VirtIoResult<()> {
        let req = ResourceAttachBacking {
            header: CtrlHeader::with_type(Command::RESOURCE_ATTACH_BACKING),
            resource_id,
            nr_entries: 1,
            addr: paddr,
            length,
            _padding: 0,
        };
        let rsp = self.request(req, CtrlHeader::default())?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn update_cursor(
        &mut self,
        resource_id: u32,
        scanout_id: u32,
        pos_x: u32,
        pos_y: u32,
        hot_x: u32,
        hot_y: u32,
        is_move: bool,
    ) -> VirtIoResult<()> {
        let req = UpdateCursor {
            header: if is_move {
                CtrlHeader::with_type(Command::MOVE_CURSOR)
            } else {
                CtrlHeader::with_type(Command::UPDATE_CURSOR)
            },
            pos: CursorPos {
                scanout_id,
                x: pos_x,
                y: pos_y,
                _padding: 0,
            },
            resource_id,
            hot_x,
            hot_y,
            _padding: 0,
        };
        self.cursor_request(req)
    }

    fn get_display_info(&mut self) -> VirtIoResult<RespDisplayInfo> {
        let info = self.request(
            CtrlHeader::with_type(Command::GET_DISPLAY_INFO),
            RespDisplayInfo::default(),
        )?;
        info.header.check_type(Command::OK_DISPLAY_INFO)?;
        Ok(info)
    }
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> Drop for VirtIOGpu<H, T> {
    fn drop(&mut self) {
        // Clear any pointers pointing to DMA regions, so the device doesn't try to access them
        // after they have been freed.
        self.transport
            .queue_unset(QUEUE_TRANSMIT)
            .expect("failed to unset transmit queue");
        self.transport
            .queue_unset(QUEUE_CURSOR)
            .expect("failed to unset cursor queue");
    }
}
