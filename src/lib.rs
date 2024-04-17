#![no_std]
extern crate alloc;

use core::{
    hint::spin_loop,
    mem::{size_of, size_of_val},
};

use alloc::{
    boxed::Box,
    collections::{BTreeSet, VecDeque},
    vec::Vec,
};
use log::{info, warn};

pub const SECTOR_SIZE: usize = 512;
pub const PAGE_SIZE: usize = 4096;
const CONFIG_OFFSET: usize = 0x100;

#[cfg(feature = "alien")]
type Res<T> = AlienResult<T>;
#[cfg(not(feature = "alien"))]
type Res<T> = Result<T, ()>;

// needs unsafe ???
pub trait SvdOps: Send + Sync {
    fn read_at(&self, off: usize) -> Res<u32>;
    fn write_at(&self, off: usize, data: u32) -> Res<()>;
}

pub struct BlkDriver<'a> {
    ops: Box<dyn SvdOps>,
    queue: VirtQueue<'a, { BlkDriver::QUEUE_SIZE }>,
}

impl<'a> BlkDriver<'a> {
    const SUPPORT_FEAT: u64 = BlkFeature::FLUSH;
    pub const QUEUE_SIZE: usize = 16;

    pub fn new(
        ops: Box<dyn SvdOps>,
        mut vq: VirtQueue<'a, { BlkDriver::QUEUE_SIZE }>,
    ) -> Res<Self> {
        let header = VirtIOHeader::default();
        header.general_init(&ops, Self::SUPPORT_FEAT)?;
        // read config
        let config = BlkConfig::default();
        let capacity = ((config.capacity_high.read(&ops)? as u64) << 32)
            | (config.capacity_low.read(&ops)? as u64);
        info!("block device size: {}KB", capacity / 2);
        // set queue
        vq.init();
        if header.is_legacy(&ops)? {
            let size = Self::QUEUE_SIZE;
            let align = PAGE_SIZE as u32;
            let pfn = (vq.desc_pa() / PAGE_SIZE) as u32;
            // if desc_pa can be divided by PAGE_SIZE
            assert_eq!(pfn as usize * PAGE_SIZE, vq.desc_pa());
            // queue index
            header.queue_sel.write(&ops, 0)?;
            let qm = header.queue_num_max.read(&ops)? as usize;
            if qm < size {
                return Err(());
            }
            header.queue_num.write(&ops, size as u32)?;
            header.legacy_queue_align.write(&ops, align)?;
            header.legacy_queue_pfn.write(&ops, pfn)?;
        } else {
            // modern interface
            todo!("modern interface do not implement yet. please use the legacy instead.");
        }
        header.general_init_end(&ops)?;
        Ok(Self { ops, queue: vq })
    }
    /// assert_eq!(buf.len() % 512, 0)
    pub fn read_blocks(&mut self, sector: usize, buf: &mut [u8]) -> Res<()> {
        assert_ne!(buf.len(), 0);
        assert_eq!(buf.len() % SECTOR_SIZE, 0);
        todo!()
    }
    /// assert_eq!(buf.len() % 512, 0)
    pub fn write_blocks(&mut self, sector: usize, buf: &[u8]) -> Res<()> {
        assert_ne!(buf.len(), 0);
        assert_eq!(buf.len() % SECTOR_SIZE, 0);
        warn!("in write : avail idx = {}", self.queue.avail.idx);
        let header = VirtIOHeader::default();
        let ops = &self.ops;

        let mut v = Vec::new();
        let req = BlkReqHeader::new(BlkReqType::Out, sector as u64);
        let resp = BlkRespStatus::NONE;
        // get the physical address of header
        v.push(Descriptor::new(
            &req as *const _ as _,
            size_of_val(&req) as _,
            DescFlag::NEXT,
        ));
        v.push(Descriptor::new(
            buf as *const _ as *const u8 as _,
            buf.len() as _,
            DescFlag::NEXT,
        ));
        v.push(Descriptor::new(
            &resp as *const _ as _,
            size_of_val(&resp) as _,
            DescFlag::WRITE,
        ));
        let token = self.queue.push(v)?;
        // notify the device
        header.queue_notify.write(ops, 0)?;
        warn!("pushed");
        // wait
        let mut counter = 0;
        while !self.queue.is_ready(token)? {
            counter += 1;
            if counter % 1000000 == 0 {
                warn!("counter : {counter}");
            }
            spin_loop();
        }
        info!("pop");
        // get resp & pop queue
        self.queue.pop(token)?;
        assert_eq!(resp, BlkRespStatus::OK);
        info!("write finish");
        Ok(())
    }
}
#[repr(C)]
#[derive(Debug)]
struct BlkReqHeader {
    type_: BlkReqType,
    reserved: u32,
    sector: u64,
}
impl BlkReqHeader {
    fn new(t: BlkReqType, sector: u64) -> Self {
        Self {
            type_: t,
            reserved: 0,
            sector,
        }
    }
}
#[repr(u32)]
#[derive(Debug)]
enum BlkReqType {
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
struct BlkRespStatus;
impl BlkRespStatus {
    /// not ready yet
    const NONE: u8 = u8::MAX;
    const OK: u8 = 0;
    /// IoErr.
    const IO_ERR: u8 = 1;
    /// Unsupported yet.
    const UNSUPPORTED: u8 = 2;
}

struct BlkConfig {
    capacity_low: ReadWrite<CONFIG_OFFSET>,
    capacity_high: ReadWrite<{ CONFIG_OFFSET + 0x4 }>,
    size_max: ReadWrite<{ CONFIG_OFFSET + 0x8 }>,
    seg_max: ReadWrite<{ CONFIG_OFFSET + 0xc }>,

    // cylinders: ReadWrite<u16>,
    // heads: ReadWrite<u8>,
    // sectors: ReadWrite<u8>,
    geometry: ReadWrite<{ CONFIG_OFFSET + 0x10 }>,

    blk_size: ReadWrite<{ CONFIG_OFFSET + 0x14 }>,

    // physical_block_exp: ReadWrite<u8>,
    // alignment_offset: ReadWrite<u8>,
    // min_io_size: ReadWrite<u16>,
    topology: ReadWrite<{ CONFIG_OFFSET + 0x18 }>,

    opt_io_size: ReadWrite<{ CONFIG_OFFSET + 0x1c }>,
    // ...
}
impl Default for BlkConfig {
    fn default() -> Self {
        Self {
            capacity_low: ReadWrite,
            capacity_high: ReadWrite,
            size_max: ReadWrite,
            seg_max: ReadWrite,
            geometry: ReadWrite,
            blk_size: ReadWrite,
            topology: ReadWrite,
            opt_io_size: ReadWrite,
        }
    }
}

/// MMIO Device Register Interface, both legacy and modern.
///
/// Ref: 4.2.2 MMIO Device Register Layout and 4.2.4 Legacy interface
struct VirtIOHeader {
    /// Magic value
    magic: ReadOnly<0>,

    /// Device version number
    ///
    /// Legacy device returns value 0x1.
    version: ReadOnly<0x4>,

    /// Virtio Subsystem Device ID
    device_id: ReadOnly<0x8>,

    /// Virtio Subsystem Vendor ID
    vendor_id: ReadOnly<0xc>,

    /// Flags representing features the device supports
    device_features: ReadOnly<0x10>,

    /// Device (host) features word selection
    device_features_sel: WriteOnly<0x14>,

    /// Reserved
    // __r1: [ReadOnly<0>; 2],

    /// Flags representing device features understood and activated by the driver
    driver_features: WriteOnly<0x20>,

    /// Activated (guest) features word selection
    driver_features_sel: WriteOnly<0x24>,

    /// Guest page size
    ///
    /// The driver writes the guest page size in bytes to the register during
    /// initialization, before any queues are used. This value should be a
    /// power of 2 and is used by the device to calculate the Guest address
    /// of the first queue page (see QueuePFN).
    legacy_guest_page_size: WriteOnly<0x28>,

    /// Reserved
    // __r2: ReadOnly<0>,

    /// Virtual queue index
    ///
    /// Writing to this register selects the virtual queue that the following
    /// operations on the QueueNumMax, QueueNum, QueueAlign and QueuePFN
    /// registers apply to. The index number of the first queue is zero (0x0).
    queue_sel: WriteOnly<0x30>,

    /// Maximum virtual queue size
    ///
    /// Reading from the register returns the maximum size of the queue the
    /// device is ready to process or zero (0x0) if the queue is not available.
    /// This applies to the queue selected by writing to QueueSel and is
    /// allowed only when QueuePFN is set to zero (0x0), so when the queue is
    /// not actively used.
    queue_num_max: ReadOnly<0x34>,

    /// Virtual queue size
    ///
    /// Queue size is the number of elements in the queue. Writing to this
    /// register notifies the device what size of the queue the driver will use.
    /// This applies to the queue selected by writing to QueueSel.
    queue_num: WriteOnly<0x38>,

    /// Used Ring alignment in the virtual queue
    ///
    /// Writing to this register notifies the device about alignment boundary
    /// of the Used Ring in bytes. This value should be a power of 2 and
    /// applies to the queue selected by writing to QueueSel.
    legacy_queue_align: WriteOnly<0x3c>,

    /// Guest physical page number of the virtual queue
    ///
    /// Writing to this register notifies the device about location of the
    /// virtual queue in the Guest’s physical address space. This value is
    /// the index number of a page starting with the queue Descriptor Table.
    /// Value zero (0x0) means physical address zero (0x00000000) and is illegal.
    /// When the driver stops using the queue it writes zero (0x0) to this
    /// register. Reading from this register returns the currently used page
    /// number of the queue, therefore a value other than zero (0x0) means that
    /// the queue is in use. Both read and write accesses apply to the queue
    /// selected by writing to QueueSel.
    legacy_queue_pfn: ReadWrite<0x40>,

    /// new interface only
    queue_ready: ReadWrite<0x44>,

    /// Reserved
    // __r3: [ReadOnly<0>; 2],

    /// Queue notifier
    queue_notify: WriteOnly<0x50>,

    /// Reserved
    // __r4: [ReadOnly<0>; 3],

    /// Interrupt status
    interrupt_status: ReadOnly<0x60>,

    /// Interrupt acknowledge
    interrupt_ack: WriteOnly<0x64>,

    /// Reserved
    // __r5: [ReadOnly<0>; 2],

    /// Device status
    ///
    /// Reading from this register returns the current device status flags.
    /// Writing non-zero values to this register sets the status flags,
    /// indicating the OS/driver progress. Writing zero (0x0) to this register
    /// triggers a device reset. The device sets QueuePFN to zero (0x0) for
    /// all queues in the device. Also see 3.1 Device Initialization.
    status: ReadWrite<0x70>,

    /// Reserved
    // __r6: [ReadOnly<0>; 3],

    // new interface only since here
    queue_desc_low: WriteOnly<0x80>,
    queue_desc_high: WriteOnly<0x84>,

    /// Reserved
    // __r7: [ReadOnly<0>; 2],
    queue_driver_low: WriteOnly<0x90>,
    queue_driver_high: WriteOnly<0x94>,

    /// Reserved
    // __r8: [ReadOnly<0>; 2],
    queue_device_low: WriteOnly<0xA0>,
    queue_device_high: WriteOnly<0xA4>,

    /// Reserved
    // __r9: [ReadOnly<0>; 21],

    // new interface
    config_generation: ReadOnly<0xfc>,
}

impl Default for VirtIOHeader {
    fn default() -> Self {
        Self {
            magic: ReadOnly,
            version: ReadOnly,
            device_id: ReadOnly,
            vendor_id: ReadOnly,
            device_features: ReadOnly,
            device_features_sel: WriteOnly,
            driver_features: WriteOnly,
            driver_features_sel: WriteOnly,
            legacy_guest_page_size: WriteOnly,
            queue_sel: WriteOnly,
            queue_num_max: ReadOnly,
            queue_num: WriteOnly,
            legacy_queue_align: WriteOnly,
            legacy_queue_pfn: ReadWrite,
            queue_ready: ReadWrite,
            queue_notify: WriteOnly,
            interrupt_status: ReadOnly,
            interrupt_ack: WriteOnly,
            status: ReadWrite,
            queue_desc_low: WriteOnly,
            queue_desc_high: WriteOnly,
            queue_driver_low: WriteOnly,
            queue_driver_high: WriteOnly,
            queue_device_low: WriteOnly,
            queue_device_high: WriteOnly,
            config_generation: ReadOnly,
        }
    }
}

impl VirtIOHeader {
    fn is_legacy(&self, ops: &Box<dyn SvdOps>) -> Res<bool> {
        Ok(self.version.read(ops)? == 1)
    }
    fn general_init(&self, ops: &Box<dyn SvdOps>, driver_feat: u64) -> Res<()> {
        if self.magic.read(ops)? != MAGIC {
            return Err(());
        }
        let version = self.version.read(ops)?;
        if version != 1 && version != 2 {
            return Err(());
        }
        if self.device_id.read(ops)? == 0 {
            return Err(());
        }
        let mut stat = 0;
        // 1. write 0 in status
        self.status.write(ops, stat)?;
        // 2. status::acknowledge -> 1
        stat |= DeviceStatus::ACKNOWLEDGE;
        // 3. status::driver -> 1
        stat |= DeviceStatus::DRIVER;
        self.status.write(ops, stat)?;
        // 4. read features & cal features
        let device_feat = self.device_features.read(ops)?;
        let ack_feat = device_feat & (driver_feat as u32); // u64?

        // 5. write features
        self.driver_features.write(ops, ack_feat)?;
        // 6. status::feature_ok -> 1
        stat |= DeviceStatus::FEATURES_OK;
        self.status.write(ops, stat)?;
        // 7. re_read status::feature_ok == 1
        let status = self.status.read(ops)?;
        if stat != status {
            return Err(());
        }
        // 8. device specific config ?????
        if version == 1 {
            self.legacy_guest_page_size.write(ops, PAGE_SIZE as u32)?;
        }
        Ok(())
    }
    fn general_init_end(&self, ops: &Box<dyn SvdOps>) -> Res<()> {
        // 9. status::driver_ok -> 1
        let stat = DeviceStatus::ACKNOWLEDGE
            | DeviceStatus::DRIVER
            | DeviceStatus::FEATURES_OK
            | DeviceStatus::DRIVER_OK;
        self.status.write(ops, stat)
    }
}

struct ReadOnly<const OFF: usize>;
struct WriteOnly<const OFF: usize>;
struct ReadWrite<const OFF: usize>;

impl<const OFF: usize> ReadOnly<OFF> {
    fn read(&self, ops: &Box<dyn SvdOps>) -> Res<u32> {
        ops.read_at(OFF)
    }
}
impl<const OFF: usize> WriteOnly<OFF> {
    fn write(&self, ops: &Box<dyn SvdOps>, data: u32) -> Res<()> {
        ops.write_at(OFF, data)
    }
}
impl<const OFF: usize> ReadWrite<OFF> {
    fn read(&self, ops: &Box<dyn SvdOps>) -> Res<u32> {
        ops.read_at(OFF)
    }
    fn write(&self, ops: &Box<dyn SvdOps>, data: u32) -> Res<()> {
        ops.write_at(OFF, data)
    }
}

const MAGIC: u32 = 0x_7472_6976;
struct DeviceStatus;
impl DeviceStatus {
    /// Indicates that the guest OS has found the device and recognized it
    /// as a valid virtio device.
    const ACKNOWLEDGE: u32 = 1;

    /// Indicates that the guest OS knows how to drive the device.
    const DRIVER: u32 = 2;

    /// Indicates that something went wrong in the guest, and it has given
    /// up on the device. This could be an internal error, or the driver
    /// didn’t like the device for some reason, or even a fatal error
    /// during device operation.
    const FAILED: u32 = 128;

    /// Indicates that the driver has acknowledged all the features it
    /// understands, and feature negotiation is complete.
    const FEATURES_OK: u32 = 8;

    /// Indicates that the driver is set up and ready to drive the device.
    const DRIVER_OK: u32 = 4;

    /// Indicates that the device has experienced an error from which it
    /// can’t recover.
    const DEVICE_NEEDS_RESET: u32 = 64;
}

struct BlkFeature;
impl BlkFeature {
    /// Device supports request barriers. (legacy)
    const BARRIER: u64 = 1 << 0;
    /// Maximum size of any single segment is in `size_max`.
    const SIZE_MAX: u64 = 1 << 1;
    /// Maximum number of segments in a request is in `seg_max`.
    const SEG_MAX: u64 = 1 << 2;
    /// Disk-style geometry specified in geometry.
    const GEOMETRY: u64 = 1 << 4;
    /// Device is read-only.
    const RO: u64 = 1 << 5;
    /// Block size of disk is in `blk_size`.
    const BLK_SIZE: u64 = 1 << 6;
    /// Device supports scsi packet commands. (legacy)
    const SCSI: u64 = 1 << 7;
    /// Cache flush command support.
    const FLUSH: u64 = 1 << 9;
    /// Device exports information on optimal I/O alignment.
    const TOPOLOGY: u64 = 1 << 10;
    /// Device can toggle its cache between writeback and writethrough modes.
    const CONFIG_WCE: u64 = 1 << 11;
    /// Device supports multiqueue.
    const MQ: u64 = 1 << 12;
    /// Device can support discard command; maximum discard sectors size in
    /// `max_discard_sectors` and maximum discard segment number in
    /// `max_discard_seg`.
    const DISCARD: u64 = 1 << 13;
    /// Device can support write zeroes command; maximum write zeroes sectors
    /// size in `max_write_zeroes_sectors` and maximum write zeroes segment
    /// number in `max_write_zeroes_seg`.
    const WRITE_ZEROES: u64 = 1 << 14;
    /// Device supports providing storage lifetime information.
    const LIFETIME: u64 = 1 << 15;
    /// Device can support the secure erase command.
    const SECURE_ERASE: u64 = 1 << 16;

    // device independent
    const NOTIFY_ON_EMPTY: u64 = 1 << 24; // legacy
    const ANY_LAYOUT: u64 = 1 << 27; // legacy
    const RING_INDIRECT_DESC: u64 = 1 << 28;
    const RING_EVENT_IDX: u64 = 1 << 29;
    const UNUSED: u64 = 1 << 30; // legacy
    const VERSION_1: u64 = 1 << 32; // detect legacy

    // the following since virtio v1.1
    const ACCESS_PLATFORM: u64 = 1 << 33;
    const RING_PACKED: u64 = 1 << 34;
    const IN_ORDER: u64 = 1 << 35;
    const ORDER_PLATFORM: u64 = 1 << 36;
    const SR_IOV: u64 = 1 << 37;
    const NOTIFICATION_DATA: u64 = 1 << 38;
}

pub struct VirtQueue<'a, const SIZE: usize> {
    desc: &'a mut [Descriptor; SIZE],
    avail: &'a mut AvailRing<SIZE>,
    used: &'a mut UsedRing<SIZE>,
    desc_pa: usize,
    // real queue, storage available descriptor indexes
    q: VecDeque<u16>,
    last_seen_used: u16,
    poped_used: BTreeSet<u16>,
}
impl<'a, const SIZE: usize> VirtQueue<'a, SIZE> {
    pub fn new(
        desc: &'a mut [Descriptor; SIZE],
        avail: &'a mut AvailRing<SIZE>,
        used: &'a mut UsedRing<SIZE>,
        desc_pa: usize,
    ) -> Res<Self> {
        let q = VecDeque::from_iter(0..SIZE as u16);
        Ok(Self {
            desc,
            avail,
            used,
            desc_pa,
            q,
            last_seen_used: 0,
            poped_used: BTreeSet::new(),
        })
    }
    fn desc_pa(&self) -> usize {
        self.desc_pa
    }
    fn avail_pa(&self) -> usize {
        // don't use `Self::avail_offset` for speed
        self.desc_pa + size_of::<Descriptor>() * SIZE
    }
    fn used_pa(&self) -> usize {
        // don't use `Self::used_offset` for speed
        self.desc_pa + align_up(size_of::<Descriptor>() * SIZE + size_of::<u16>() * (SIZE + 3))
    }
    pub fn avail_offset() -> Res<usize> {
        Ok(size_of::<Descriptor>() * SIZE)
    }
    pub fn used_offset() -> Res<usize> {
        Ok(align_up(size_of::<Descriptor>() * SIZE) + size_of::<AvailRing<SIZE>>())
    }
    pub fn total_size() -> Res<usize> {
        Ok(
            align_up(size_of::<Descriptor>() * SIZE + size_of::<AvailRing<SIZE>>())
                + align_up(size_of::<UsedRing<SIZE>>()),
        )
    }
    fn init(&mut self) {
        self.avail.init();
    }
    fn push(&mut self, mut data: Vec<Descriptor>) -> Res<u16> {
        assert_ne!(data.len(), 0);
        if self.q.len() < data.len() {
            return Err(());
        }
        let mut last = None;
        for d in data.iter_mut().rev() {
            let id = self.q.pop_front().unwrap();

            if let Some(nex) = last {
                d.next = nex;
            }
            warn!(
                "buffer len : {} id={} nex_flag={}, nex={} | idx = {}",
                d.len,
                id,
                d.flags & DescFlag::NEXT,
                d.next,
                self.avail.idx
            );
            //  write desc to self.desc
            self.desc[id as usize % SIZE] = *d;
            last = Some(id);
        }
        let head = last.unwrap();
        // change the avail ring
        self.avail.push(head)?;
        Ok(head)
    }
    fn is_ready(&self, id: u16) -> Res<bool> {
        if self.last_seen_used == self.used.idx {
            return Ok(false);
        }
        for i in self.last_seen_used..self.used.idx {
            if self.used.ring[i as usize % SIZE].id == id as u32 {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn pop(&mut self, id: u16) -> Res<()> {
        assert!(self.last_seen_used < self.used.idx);
        let mut header = self.last_seen_used - 1;
        for i in self.last_seen_used..self.used.idx {
            if self.used.ring[i as usize % SIZE].id == id as u32 {
                // pop this
                header = i;
                break;
            }
        }
        assert_ne!(header, self.last_seen_used - 1);
        self.poped_used.insert(header);
        let mut now = self.used.ring[header as usize].id as usize;
        while (self.desc[now].flags & DescFlag::NEXT) != 0 {
            now = self.desc[now as usize].next as _;
        }
        // update last_seen_used
        while self.poped_used.contains(&self.last_seen_used) {
            self.poped_used.remove(&self.last_seen_used);
            self.last_seen_used += 1;
        }
        // return value
        Ok(())
    }
}

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}
impl Default for Descriptor {
    fn default() -> Self {
        Self {
            addr: Default::default(),
            len: Default::default(),
            flags: Default::default(),
            next: Default::default(),
        }
    }
}
impl Descriptor {
    fn new(addr: u64, len: u32, flags: u16) -> Self {
        Self {
            addr,
            len,
            flags,
            next: 0,
        }
    }
}
struct DescFlag;
impl DescFlag {
    const NEXT: u16 = 1;
    const WRITE: u16 = 2;
    const INDIRECT: u16 = 4;
}
#[repr(C)]
#[derive(Debug)]
pub struct AvailRing<const SIZE: usize> {
    flags: u16,
    /// A driver MUST NOT decrement the idx.
    idx: u16,
    ring: [u16; SIZE],
    /// Only used if `VIRTIO_F_EVENT_IDX` is negotiated.
    used_event: u16,
}
impl<const SIZE: usize> AvailRing<SIZE> {
    // fn new() -> Self {
    //     Self {
    //         flags: Default::default(),
    //         idx: Default::default(),
    //         ring: [Default::default(); SIZE],
    //         used_event: Default::default(),
    //     }
    // }
    fn init(&mut self) {
        self.flags = 0;
        self.idx = 0;
    }
    fn push(&mut self, id: u16) -> Res<u16> {
        // have enough space, because (avail ring's len == desc's)
        self.ring[self.idx as usize % SIZE] = id;
        warn!(
            "avail pushed: id={} header={}",
            self.idx,
            self.ring[self.idx as usize % SIZE]
        );
        self.idx = self.idx + 1;
        Ok(self.idx - 1)
    }
}
#[repr(C)]
#[derive(Debug)]
pub struct UsedRing<const SIZE: usize> {
    flags: u16,
    idx: u16,
    ring: [UsedElem; SIZE],
    /// Only used if `VIRTIO_F_EVENT_IDX` is negotiated.
    avail_event: u16,
}
impl<const SIZE: usize> UsedRing<SIZE> {
    // fn new() -> Self {
    //     Self {
    //         flags: Default::default(),
    //         idx: Default::default(),
    //         ring: [Default::default(); SIZE],
    //         avail_event: Default::default(),
    //     }
    // }
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UsedElem {
    id: u32,
    len: u32,
}
// impl Default for UsedElem {
//     fn default() -> Self {
//         Self {
//             id: Default::default(),
//             len: Default::default(),
//         }
//     }
// }

/// Align `size` up to a page.
fn align_up(size: usize) -> usize {
    (size + PAGE_SIZE) & !(PAGE_SIZE - 1)
}

/// The number of pages required to store `size` bytes, rounded up to a whole number of pages.
fn pages(size: usize) -> usize {
    (size + PAGE_SIZE - 1) / PAGE_SIZE
}
