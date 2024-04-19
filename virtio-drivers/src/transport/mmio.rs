use alloc::boxed::Box;
use crate::{ PAGE_SIZE};
use crate::error::{VirtIoError, VirtIoResult};
use crate::hal::VirtIoDeviceIo;
use crate::transport::DeviceStatus;
use crate::volatile::{ReadOnly, ReadVolatile, ReadWrite, WriteOnly, WriteVolatile};
pub const MAGIC: u32 = 0x_7472_6976;
pub const CONFIG_OFFSET: usize = 0x100;

/// MMIO Device Register Interface, both legacy and modern.
///
/// Ref: 4.2.2 MMIO Device Register Layout and 4.2.4 Legacy interface
pub struct VirtIOHeader {
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
    pub(crate) queue_sel: WriteOnly<0x30>,

    /// Maximum virtual queue size
    ///
    /// Reading from the register returns the maximum size of the queue the
    /// device is ready to process or zero (0x0) if the queue is not available.
    /// This applies to the queue selected by writing to QueueSel and is
    /// allowed only when QueuePFN is set to zero (0x0), so when the queue is
    /// not actively used.
    pub(crate) queue_num_max: ReadOnly<0x34>,

    /// Virtual queue size
    ///
    /// Queue size is the number of elements in the queue. Writing to this
    /// register notifies the device what size of the queue the driver will use.
    /// This applies to the queue selected by writing to QueueSel.
    pub(crate) queue_num: WriteOnly<0x38>,

    /// Used Ring alignment in the virtual queue
    ///
    /// Writing to this register notifies the device about alignment boundary
    /// of the Used Ring in bytes. This value should be a power of 2 and
    /// applies to the queue selected by writing to QueueSel.
    pub(crate) legacy_queue_align: WriteOnly<0x3c>,

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
    pub(crate) legacy_queue_pfn: ReadWrite<0x40>,

    /// new interface only
    queue_ready: ReadWrite<0x44>,

    /// Reserved
    // __r3: [ReadOnly<0>; 2],

    /// Queue notifier
    pub(crate) queue_notify: WriteOnly<0x50>,

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

    pub(crate) fn is_legacy(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<bool> {
        Ok(self.version.read_volatile_u32(io_region)? == 1)
    }
    pub(crate) fn general_init(&self, io_region: &Box<dyn VirtIoDeviceIo>, driver_feat: u64) -> VirtIoResult<()> {
        if self.magic.read_volatile_u32(io_region)? != MAGIC {
            return Err(VirtIoError::InvalidParam)
        }
        let version = self.version.read_volatile_u32(io_region)?;
        if version != 1 && version != 2 {
            return Err(VirtIoError::InvalidParam)
        }
        if self.device_id.read_volatile_u32(io_region)? == 0 {
            return Err(VirtIoError::InvalidParam);
        }
        let mut stat = DeviceStatus::empty();
        // 1. write 0 in status
        self.status.write_volatile_u32(stat.bits(),io_region )?;
        // 2. status::acknowledge -> 1
        // 3. status::driver -> 1
        stat |= DeviceStatus::ACKNOWLEDGE | DeviceStatus::DRIVER;
        self.status.write_volatile_u32( stat.bits(),io_region)?;
        // 4. read features & cal features
        let device_feat = self.device_features.read_volatile_u32(io_region)?;
        let ack_feat = device_feat & driver_feat as u32;

        // 5. write features
        self.driver_features.write_volatile_u32(ack_feat,io_region)?;
        // 6. status::feature_ok -> 1
        stat |= DeviceStatus::FEATURES_OK;
        self.status.write_volatile_u32(stat.bits(),io_region)?;
        // 7. re_read status::feature_ok == 1
        let status = self.status.read_volatile_u32(io_region)?;
        if stat.bits() != status {
            return Err(VirtIoError::InvalidParam)
        }
        // 8. device specific config ?????
        if version == 1 {
            self.legacy_guest_page_size.write_volatile_u32(PAGE_SIZE as u32,io_region)?;
        }
        Ok(())
    }
    pub(crate) fn general_init_end(&self, io_region: &Box<dyn VirtIoDeviceIo>) -> VirtIoResult<()> {
        // 9. status::driver_ok -> 1
        let stat = DeviceStatus::ACKNOWLEDGE
            | DeviceStatus::DRIVER
            | DeviceStatus::FEATURES_OK
            | DeviceStatus::DRIVER_OK;
        self.status.write_volatile_u32( stat.bits(),io_region)
    }
}