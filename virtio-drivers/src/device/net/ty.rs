use crate::common::Array;
use crate::error::VirtIoResult;
use crate::transport::mmio::CONFIG_OFFSET;
use core::mem::size_of;

use crate::volatile::ReadOnly;
use bitflags::bitflags;

pub const MAX_BUFFER_LEN: usize = 65535;
pub const MIN_BUFFER_LEN: usize = 1526;
pub const NET_HDR_SIZE: usize = core::mem::size_of::<VirtioNetHdr>();

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct Features: u64 {
        /// Device handles packets with partial checksum.
        /// This "checksum offload" is a common feature on modern network cards.
        const CSUM = 1 << 0;
        /// Driver handles packets with partial checksum.
        const GUEST_CSUM = 1 << 1;
        /// Control channel offloads reconfiguration support.
        const CTRL_GUEST_OFFLOADS = 1 << 2;
        /// Device maximum MTU reporting is supported.
        ///
        /// If offered by the device, device advises driver about the value of
        /// its maximum MTU. If negotiated, the driver uses mtu as the maximum
        /// MTU value.
        const MTU = 1 << 3;
        /// Device has given MAC address.
        const MAC = 1 << 5;
        /// Device handles packets with any GSO type. (legacy)
        const GSO = 1 << 6;
        /// Driver can receive TSOv4.
        const GUEST_TSO4 = 1 << 7;
        /// Driver can receive TSOv6.
        const GUEST_TSO6 = 1 << 8;
        /// Driver can receive TSO with ECN.
        const GUEST_ECN = 1 << 9;
        /// Driver can receive UFO.
        const GUEST_UFO = 1 << 10;
        /// Device can receive TSOv4.
        const HOST_TSO4 = 1 << 11;
        /// Device can receive TSOv6.
        const HOST_TSO6 = 1 << 12;
        /// Device can receive TSO with ECN.
        const HOST_ECN = 1 << 13;
        /// Device can receive UFO.
        const HOST_UFO = 1 << 14;
        /// Driver can merge receive buffers.
        const MRG_RXBUF = 1 << 15;
        /// Configuration status field is available.
        const STATUS = 1 << 16;
        /// Control channel is available.
        const CTRL_VQ = 1 << 17;
        /// Control channel RX mode support.
        const CTRL_RX = 1 << 18;
        /// Control channel VLAN filtering.
        const CTRL_VLAN = 1 << 19;
        ///
        const CTRL_RX_EXTRA = 1 << 20;
        /// Driver can send gratuitous packets.
        const GUEST_ANNOUNCE = 1 << 21;
        /// Device supports multiqueue with automatic receive steering.
        const MQ = 1 << 22;
        /// Set MAC address through control channel.
        const CTL_MAC_ADDR = 1 << 23;

        // device independent
        const RING_INDIRECT_DESC = 1 << 28;
        const RING_EVENT_IDX = 1 << 29;
        const VERSION_1 = 1 << 32; // legacy
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct Status: u16 {
        const LINK_UP = 1;
        const ANNOUNCE = 2;
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct InterruptStatus : u32 {
        const USED_RING_UPDATE = 1 << 0;
        const CONFIGURATION_CHANGE = 1 << 1;
    }
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct NetConfig {
    pub mac: ReadOnly<CONFIG_OFFSET, EthernetAddress>,
    pub status: ReadOnly<{ CONFIG_OFFSET + 6 }, u16>,
    pub max_virtqueue_pairs: ReadOnly<{ CONFIG_OFFSET + 8 }, u16>,
    pub mtu: ReadOnly<{ CONFIG_OFFSET + 10 }, u16>,
}

pub type EthernetAddress = Array<6, u8>;

/// VirtIO 5.1.6 Device Operation:
///
/// Packets are transmitted by placing them in the transmitq1. . .transmitqN,
/// and buffers for incoming packets are placed in the receiveq1. . .receiveqN.
/// In each case, the packet itself is preceded by a header.
#[repr(C)]
#[derive(Debug, Default)]
pub struct VirtioNetHdr {
    pub flags: Flags,
    pub gso_type: GsoType,
    pub hdr_len: u16, // cannot rely on this
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    // num_buffers: u16, // only available when the feature MRG_RXBUF is negotiated.
    // payload starts from here
}
impl VirtioNetHdr {
    pub fn write_to(&self, target: &mut [u8]) {
        assert!(target.len() >= size_of::<Self>());
        target[0] = self.flags.0;
        target[1] = self.gso_type.0;
        // (&mut target[2..4]).copy_from_slice(&self.hdr_len.to_le_bytes());
        // (&mut target[4..6]).copy_from_slice(&self.gso_size.to_le_bytes());
        // (&mut target[6..8]).copy_from_slice(&self.csum_start.to_le_bytes());
        // (&mut target[8..10]).copy_from_slice(&self.csum_offset.to_le_bytes());

        target[2] = self.hdr_len as _;
        target[3] = (self.hdr_len >> 8) as _;
        target[4] = self.gso_size as _;
        target[5] = (self.gso_size >> 8) as _;
        target[6] = self.csum_start as _;
        target[7] = (self.csum_start >> 8) as _;
        target[8] = self.csum_offset as _;
        target[9] = (self.csum_offset >> 8) as _;
    }
    pub fn equal(&self, target: &[u8]) -> VirtIoResult<bool> {
        assert!(target.len() >= size_of::<Self>());
        let mut flag = true;
        flag &= target[0] == self.flags.0;
        flag &= target[1] == self.gso_type.0;
        flag &= target[2] == self.hdr_len as _;
        flag &= target[3] == (self.hdr_len >> 8) as _;
        flag &= target[4] == self.gso_size as _;
        flag &= target[5] == (self.gso_size >> 8) as _;
        flag &= target[6] == self.csum_start as _;
        flag &= target[7] == (self.csum_start >> 8) as _;
        flag &= target[8] == self.csum_offset as _;
        flag &= target[9] == (self.csum_offset >> 8) as _;
        Ok(flag)
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct Flags(u8);

bitflags! {
    impl Flags: u8 {
        const NEEDS_CSUM = 1;
        const DATA_VALID = 2;
        const RSC_INFO   = 4;
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub struct GsoType(u8);

impl GsoType {
    const NONE: GsoType = GsoType(0);
    const TCPV4: GsoType = GsoType(1);
    const UDP: GsoType = GsoType(3);
    const TCPV6: GsoType = GsoType(4);
    const ECN: GsoType = GsoType(0x80);
}

pub const QUEUE_RECEIVE: u16 = 0;
pub const QUEUE_TRANSMIT: u16 = 1;
pub const SUPPORTED_FEATURES: Features = Features::MAC.union(Features::STATUS);
// .union(Features::RING_EVENT_IDX);
