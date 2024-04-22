use crate::error::{VirtIoError, VirtIoResult};
use crate::transport::mmio::CONFIG_OFFSET;
use crate::volatile::{ReadOnly, ReadWrite, WriteOnly};
use bitflags::bitflags;
#[repr(C)]
#[derive(Debug, Default)]
pub struct GpuConfig {
    /// Signals pending events to the driver。
    pub(crate) events_read: ReadOnly<CONFIG_OFFSET, u32>,

    /// Clears pending events in the device.
    pub(crate) events_clear: WriteOnly<{ CONFIG_OFFSET + 4 }, u32>,

    /// Specifies the maximum number of scanouts supported by the device.
    ///
    /// Minimum value is 1, maximum value is 16.
    pub(crate) num_scanouts: ReadWrite<{ CONFIG_OFFSET + 8 }, u32>,
    pub(crate) num_capsets: ReadWrite<{ CONFIG_OFFSET + 12 }, u32>,
}

/// Display configuration has changed.
const EVENT_DISPLAY: u32 = 1 << 0;

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct Features: u64 {
        /// virgl 3D mode is supported.
        const VIRGL                 = 1 << 0;
        /// EDID is supported.
        const EDID                  = 1 << 1;

        // device independent
        const NOTIFY_ON_EMPTY       = 1 << 24; // legacy
        const ANY_LAYOUT            = 1 << 27; // legacy
        const RING_INDIRECT_DESC    = 1 << 28;
        const RING_EVENT_IDX        = 1 << 29;
        const UNUSED                = 1 << 30; // legacy
        const VERSION_1             = 1 << 32; // detect legacy

        // since virtio v1.1
        const ACCESS_PLATFORM       = 1 << 33;
        const RING_PACKED           = 1 << 34;
        const IN_ORDER              = 1 << 35;
        const ORDER_PLATFORM        = 1 << 36;
        const SR_IOV                = 1 << 37;
        const NOTIFICATION_DATA     = 1 << 38;
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Command(u32);

impl Command {
    pub(super) const GET_DISPLAY_INFO: Command = Command(0x100);
    pub(super) const RESOURCE_CREATE_2D: Command = Command(0x101);
    pub(super) const RESOURCE_UNREF: Command = Command(0x102);
    pub(super) const SET_SCANOUT: Command = Command(0x103);
    pub(super) const RESOURCE_FLUSH: Command = Command(0x104);
    pub(super) const TRANSFER_TO_HOST_2D: Command = Command(0x105);
    pub(super) const RESOURCE_ATTACH_BACKING: Command = Command(0x106);
    pub(super) const RESOURCE_DETACH_BACKING: Command = Command(0x107);
    pub(super) const GET_CAPSET_INFO: Command = Command(0x108);
    pub(super) const GET_CAPSET: Command = Command(0x109);
    pub(super) const GET_EDID: Command = Command(0x10a);

    pub(super) const UPDATE_CURSOR: Command = Command(0x300);
    pub(super) const MOVE_CURSOR: Command = Command(0x301);

    pub(super) const OK_NODATA: Command = Command(0x1100);
    pub(super) const OK_DISPLAY_INFO: Command = Command(0x1101);
    pub(super) const OK_CAPSET_INFO: Command = Command(0x1102);
    pub(super) const OK_CAPSET: Command = Command(0x1103);
    pub(super) const OK_EDID: Command = Command(0x1104);

    pub(super) const ERR_UNSPEC: Command = Command(0x1200);
    pub(super) const ERR_OUT_OF_MEMORY: Command = Command(0x1201);
    pub(super) const ERR_INVALID_SCANOUT_ID: Command = Command(0x1202);
}

impl Default for Command {
    fn default() -> Self {
        Command(0)
    }
}

const GPU_FLAG_FENCE: u32 = 1 << 0;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CtrlHeader {
    hdr_type: Command,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    _padding: u32,
}

impl CtrlHeader {
    pub(super) fn with_type(hdr_type: Command) -> CtrlHeader {
        CtrlHeader {
            hdr_type,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            _padding: 0,
        }
    }

    /// Return error if the type is not same as expected.
    pub(super) fn check_type(&self, expected: Command) -> VirtIoResult<()> {
        if self.hdr_type == expected {
            Ok(())
        } else {
            Err(VirtIoError::IoError)
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct Rect {
    x: u32,
    y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct RespDisplayInfo {
    pub(super) header: CtrlHeader,
    pub(super) rect: Rect,
    enabled: u32,
    flags: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct ResourceCreate2D {
    pub(crate) header: CtrlHeader,
    pub(crate) resource_id: u32,
    pub(crate) format: Format,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[repr(u32)]
#[derive(Debug)]
pub enum Format {
    B8G8R8A8UNORM = 1,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct ResourceAttachBacking {
    pub(crate) header: CtrlHeader,
    pub(crate) resource_id: u32,
    pub(crate) nr_entries: u32, // always 1
    pub(crate) addr: u64,
    pub(crate) length: u32,
    pub(crate) _padding: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct SetScanout {
    pub(crate) header: CtrlHeader,
    pub(crate) rect: Rect,
    pub(crate) scanout_id: u32,
    pub(crate) resource_id: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct TransferToHost2D {
    pub(crate) header: CtrlHeader,
    pub(crate) rect: Rect,
    pub(crate) offset: u64,
    pub(crate) resource_id: u32,
    pub(crate) _padding: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct ResourceFlush {
    pub(crate) header: CtrlHeader,
    pub(crate) rect: Rect,
    pub(crate) resource_id: u32,
    pub(crate) _padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CursorPos {
    pub(crate) scanout_id: u32,
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) _padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UpdateCursor {
    pub(crate) header: CtrlHeader,
    pub(crate) pos: CursorPos,
    pub(crate) resource_id: u32,
    pub(crate) hot_x: u32,
    pub(crate) hot_y: u32,
    pub(crate) _padding: u32,
}

pub const QUEUE_TRANSMIT: u16 = 0;
pub const QUEUE_CURSOR: u16 = 1;

pub const SCANOUT_ID: u32 = 0;
pub const RESOURCE_ID_FB: u32 = 0xbabe;
pub const RESOURCE_ID_CURSOR: u32 = 0xdade;

pub const CURSOR_RECT: Rect = Rect {
    x: 0,
    y: 0,
    width: 64,
    height: 64,
};
