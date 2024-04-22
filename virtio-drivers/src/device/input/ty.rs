use crate::common::Array;
use crate::transport::mmio::CONFIG_OFFSET;
use crate::volatile::{ReadOnly, WriteOnly};
use bitflags::bitflags;

/// Select value used for [`VirtIOInput::query_config_select()`].
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum InputConfigSelect {
    Unset = 0x0,
    /// Returns the name of the device, in u.string. subsel is zero.
    IdName = 0x01,
    /// Returns the serial number of the device, in u.string. subsel is zero.
    IdSerial = 0x02,
    /// Returns ID information of the device, in u.ids. subsel is zero.
    IdDevids = 0x03,
    /// Returns input properties of the device, in u.bitmap. subsel is zero.
    /// Individual bits in the bitmap correspond to INPUT_PROP_* constants used
    /// by the underlying evdev implementation.
    PropBits = 0x10,
    /// subsel specifies the event type using EV_* constants in the underlying
    /// evdev implementation. If size is non-zero the event type is supported
    /// and a bitmap of supported event codes is returned in u.bitmap. Individual
    /// bits in the bitmap correspond to implementation-defined input event codes,
    /// for example keys or pointing device axes.
    EvBits = 0x11,
    /// subsel specifies the absolute axis using ABS_* constants in the underlying
    /// evdev implementation. Information about the axis will be returned in u.abs.
    AbsInfo = 0x12,
}

#[repr(C)]
#[derive(Debug)]
struct AbsInfo {
    min: u32,
    max: u32,
    fuzz: u32,
    flat: u32,
    res: u32,
}

#[repr(C)]
#[derive(Debug)]
struct DevIDs {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
#[derive(Debug, Default)]
pub(crate) struct InputConfig {
    pub(crate) select: WriteOnly<CONFIG_OFFSET, u8>, // 0-1
    pub(crate) subsel: WriteOnly<{ CONFIG_OFFSET + 0x1 }, u8>, // 1-2
    pub(crate) size: ReadOnly<{ CONFIG_OFFSET + 0x2 }, u8>, // 2-3
    // _reversed: [ReadOnly<u8>; 5],               // 3-8
    pub(crate) data: ReadOnly<0x8, Array<128, u8>>, // 8-136
}

// impl Default for InputConfig {
//     fn default() -> Self {
//         Self {
//             select: Default::default(),
//             subsel: Default::default(),
//             size: Default::default(),
//         }
//     }
// }

/// Both queues use the same `virtio_input_event` struct. `type`, `code` and `value`
/// are filled according to the Linux input layer (evdev) interface.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InputEvent {
    /// Event type.
    pub event_type: u16,
    /// Event code.
    pub code: u16,
    /// Event value.
    pub value: u32,
}

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub(crate) struct InputFeature: u64 {
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
