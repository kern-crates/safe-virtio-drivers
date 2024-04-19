use bitflags::bitflags;
use crate::error::{VirtIoError, VirtIoResult};
use crate::volatile::ReadWrite;
use crate::transport::mmio::CONFIG_OFFSET;

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct BlkFeature: u64 {
        /// Device supports request barriers. (legacy)
        const BARRIER       = 1 << 0;
        /// Maximum size of any single segment is in `size_max`.
        const SIZE_MAX      = 1 << 1;
        /// Maximum number of segments in a request is in `seg_max`.
        const SEG_MAX       = 1 << 2;
        /// Disk-style geometry specified in geometry.
        const GEOMETRY      = 1 << 4;
        /// Device is read-only.
        const RO            = 1 << 5;
        /// Block size of disk is in `blk_size`.
        const BLK_SIZE      = 1 << 6;
        /// Device supports scsi packet commands. (legacy)
        const SCSI          = 1 << 7;
        /// Cache flush command support.
        const FLUSH         = 1 << 9;
        /// Device exports information on optimal I/O alignment.
        const TOPOLOGY      = 1 << 10;
        /// Device can toggle its cache between writeback and writethrough modes.
        const CONFIG_WCE    = 1 << 11;
        /// Device supports multiqueue.
        const MQ            = 1 << 12;
        /// Device can support discard command, maximum discard sectors size in
        /// `max_discard_sectors` and maximum discard segment number in
        /// `max_discard_seg`.
        const DISCARD       = 1 << 13;
        /// Device can support write zeroes command, maximum write zeroes sectors
        /// size in `max_write_zeroes_sectors` and maximum write zeroes segment
        /// number in `max_write_zeroes_seg`.
        const WRITE_ZEROES  = 1 << 14;
        /// Device supports providing storage lifetime information.
        const LIFETIME      = 1 << 15;
        /// Device can support the secure erase command.
        const SECURE_ERASE  = 1 << 16;

        // device independent
        const NOTIFY_ON_EMPTY       = 1 << 24; // legacy
        const ANY_LAYOUT            = 1 << 27; // legacy
        const RING_INDIRECT_DESC    = 1 << 28;
        const RING_EVENT_IDX        = 1 << 29;
        const UNUSED                = 1 << 30; // legacy
        const VERSION_1             = 1 << 32; // detect legacy

        // the following since virtio v1.1
        const ACCESS_PLATFORM       = 1 << 33;
        const RING_PACKED           = 1 << 34;
        const IN_ORDER              = 1 << 35;
        const ORDER_PLATFORM        = 1 << 36;
        const SR_IOV                = 1 << 37;
        const NOTIFICATION_DATA     = 1 << 38;
    }
}

#[repr(u32)]
#[derive(Debug)]
pub enum BlkReqType {
    /// read
    In = 0,
    /// write
    Out = 1,
    Flush = 4,
    GetId = 8,
    GetLifetime = 10,
    Discard = 11,
    WriteZeroes = 13,
    SecureErase = 14,
}

#[repr(C)]
#[derive(Debug)]
pub struct BlkReqHeader {
    type_: BlkReqType,
    reserved: u32,
    sector: u64,
}
impl BlkReqHeader {
    pub fn new(t: BlkReqType, sector: u64) -> Self {
        Self {
            type_: t,
            reserved: 0,
            sector,
        }
    }
}

#[repr(C)]
#[derive(Debug,Eq, PartialEq)]
pub struct BlkRespStatus(u8);

impl BlkRespStatus {
    /// Ok.
    pub const OK: BlkRespStatus = BlkRespStatus(0);
    /// IoErr.
    pub const IO_ERR: BlkRespStatus = BlkRespStatus(1);
    /// Unsupported yet.
    pub const UNSUPPORTED: BlkRespStatus = BlkRespStatus(2);
    /// Not ready.
    pub const NOT_READY: BlkRespStatus = BlkRespStatus(3);
}

impl Default for BlkRespStatus{
    fn default() -> Self {
        Self::NOT_READY
    }
}

impl From<BlkRespStatus> for VirtIoResult<()> {
    fn from(status: BlkRespStatus) -> Self {
        match status {
            BlkRespStatus::OK => Ok(()),
            BlkRespStatus::IO_ERR => Err(VirtIoError::IoError),
            BlkRespStatus::UNSUPPORTED => Err(VirtIoError::Unsupported),
            BlkRespStatus::NOT_READY => Err(VirtIoError::NotReady),
            _ => Err(VirtIoError::IoError),
        }
    }
}



#[derive(Debug,Default)]
pub struct BlkConfig {
    pub(super) capacity_low: ReadWrite<CONFIG_OFFSET>,
    pub(super) capacity_high: ReadWrite<{ CONFIG_OFFSET + 0x4 }>,
    pub(super) size_max: ReadWrite<{ CONFIG_OFFSET + 0x8 }>,
    pub(super) seg_max: ReadWrite<{ CONFIG_OFFSET + 0xc }>,
    // cylinders: ReadWrite<{ CONFIG_OFFSET +  }>,
    // heads: ReadWrite<u8>,
    // sectors: ReadWrite<u8>,
    pub(super) geometry: ReadWrite<{ CONFIG_OFFSET + 0x10 }>,
    pub(super) blk_size: ReadWrite<{ CONFIG_OFFSET + 0x14 }>,
    // physical_block_exp: ReadWrite<u8>,
    // alignment_offset: ReadWrite<u8>,
    // min_io_size: ReadWrite<u16>,
    pub(super) topology: ReadWrite<{ CONFIG_OFFSET + 0x18 }>,
    pub(super) opt_io_size: ReadWrite<{ CONFIG_OFFSET + 0x1c }>,
    // ...
}

