use crate::transport::mmio::CONFIG_OFFSET;
use crate::volatile::{ReadOnly, WriteOnly};
use bitflags::bitflags;
#[derive(Debug, Default)]
pub struct ConsoleConfig {
    pub(super) cols: ReadOnly<CONFIG_OFFSET>,
    pub(super) rows: ReadOnly<{ CONFIG_OFFSET + 2 }>,
    pub(super) max_nr_ports: ReadOnly<{ CONFIG_OFFSET + 4 }>,
    pub(super) emerg_wr: WriteOnly<{ CONFIG_OFFSET + 8 }>,
}

/// Information about a console device, read from its configuration space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsoleInfo {
    /// The console height in characters.
    pub rows: u16,
    /// The console width in characters.
    pub columns: u16,
    /// The maxumum number of ports supported by the console device.
    pub max_ports: u32,
}

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct ConsoleFeatures: u64 {
        const SIZE                  = 1 << 0;
        const MULTIPORT             = 1 << 1;
        const EMERG_WRITE           = 1 << 2;

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
