use crate::error::{MmioError, VirtIoError, VirtIoResult};
use crate::hal::VirtIoDeviceIo;
use crate::queue::Descriptor;
use crate::transport::{DeviceStatus, DeviceType, Transport};
use crate::volatile::{ReadOnly, ReadVolatile, ReadWrite, WriteOnly, WriteVolatile};
use crate::{align_up, PhysAddr, PAGE_SIZE};
use alloc::boxed::Box;
use core::mem::size_of;

pub const MAGIC: u32 = 0x_7472_6976;
pub const CONFIG_OFFSET: usize = 0x100;

/// MMIO Device Register Interface, both legacy and modern.
///
/// Ref: 4.2.2 MMIO Device Register Layout and 4.2.4 Legacy interface
#[derive(Debug, Default)]
pub struct VirtIOHeader {
    /// Magic value
    magic: ReadOnly<0, u32>,

    /// Device version number
    ///
    /// Legacy device returns value 0x1.
    version: ReadOnly<0x4, u32>,

    /// Virtio Subsystem Device ID
    device_id: ReadOnly<0x8, u32>,

    /// Virtio Subsystem Vendor ID
    vendor_id: ReadOnly<0xc, u32>,

    /// Flags representing features the device supports
    device_features: ReadOnly<0x10, u32>,

    /// Device (host) features word selection
    device_features_sel: WriteOnly<0x14, u32>,

    /// Reserved
    // __r1: [ReadOnly<0>; 2],

    /// Flags representing device features understood and activated by the driver
    driver_features: WriteOnly<0x20, u32>,

    /// Activated (guest) features word selection
    driver_features_sel: WriteOnly<0x24, u32>,

    /// Guest page size
    ///
    /// The driver writes the guest page size in bytes to the register during
    /// initialization, before any queues are used. This value should be a
    /// power of 2 and is used by the device to calculate the Guest address
    /// of the first queue page (see QueuePFN).
    legacy_guest_page_size: WriteOnly<0x28, u32>,

    /// Reserved
    // __r2: ReadOnly<0>,

    /// Virtual queue index
    ///
    /// Writing to this register selects the virtual queue that the following
    /// operations on the QueueNumMax, QueueNum, QueueAlign and QueuePFN
    /// registers apply to. The index number of the first queue is zero (0x0).
    pub(crate) queue_sel: WriteOnly<0x30, u32>,

    /// Maximum virtual queue size
    ///
    /// Reading from the register returns the maximum size of the queue the
    /// device is ready to process or zero (0x0) if the queue is not available.
    /// This applies to the queue selected by writing to QueueSel and is
    /// allowed only when QueuePFN is set to zero (0x0), so when the queue is
    /// not actively used.
    pub(crate) queue_num_max: ReadOnly<0x34, u32>,

    /// Virtual queue size
    ///
    /// Queue size is the number of elements in the queue. Writing to this
    /// register notifies the device what size of the queue the driver will use.
    /// This applies to the queue selected by writing to QueueSel.
    pub(crate) queue_num: WriteOnly<0x38, u32>,

    /// Used Ring alignment in the virtual queue
    ///
    /// Writing to this register notifies the device about alignment boundary
    /// of the Used Ring in bytes. This value should be a power of 2 and
    /// applies to the queue selected by writing to QueueSel.
    pub(crate) legacy_queue_align: WriteOnly<0x3c, u32>,

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
    pub(crate) legacy_queue_pfn: ReadWrite<0x40, u32>,

    /// new interface only
    queue_ready: ReadWrite<0x44, u32>,

    /// Reserved
    // __r3: [ReadOnly<0>; 2],

    /// Queue notifier
    pub(crate) queue_notify: WriteOnly<0x50, u32>,

    /// Reserved
    // __r4: [ReadOnly<0>; 3],

    /// Interrupt status
    interrupt_status: ReadOnly<0x60, u32>,

    /// Interrupt acknowledge
    interrupt_ack: WriteOnly<0x64, u32>,

    /// Reserved
    // __r5: [ReadOnly<0>; 2],

    /// Device status
    ///
    /// Reading from this register returns the current device status flags.
    /// Writing non-zero values to this register sets the status flags,
    /// indicating the OS/driver progress. Writing zero (0x0) to this register
    /// triggers a device reset. The device sets QueuePFN to zero (0x0) for
    /// all queues in the device. Also see 3.1 Device Initialization.
    status: ReadWrite<0x70, u32>,

    /// Reserved
    // __r6: [ReadOnly<0>; 3],

    // new interface only since here
    // queue_desc_low: WriteOnly<0x80, u32>,
    // queue_desc_high: WriteOnly<0x84, u32>,
    queue_desc: WriteOnly<0x80, u64>,

    /// Reserved
    // __r7: [ReadOnly<0>; 2],
    // queue_driver_low: WriteOnly<0x90, u32>,
    // queue_driver_high: WriteOnly<0x94, u32>,
    queue_driver: WriteOnly<0x90, u64>,

    /// Reserved
    // __r8: [ReadOnly<0>; 2],
    // queue_device_low: WriteOnly<0xA0, u32>,
    // queue_device_high: WriteOnly<0xA4, u32>,
    queue_device: WriteOnly<0xA0, u64>,

    /// Reserved
    // __r9: [ReadOnly<0>; 21],

    // new interface
    config_generation: ReadOnly<0xfc, u32>,
}

// impl Default for VirtIOHeader {
//     fn default() -> Self {
//         Self {
//             magic: Default::default(),
//             version: Default::default(),
//             device_id: Default::default(),
//             vendor_id: Default::default(),
//             device_features: Default::default(),
//             device_features_sel: Default::default(),
//             driver_features: Default::default(),
//             driver_features_sel: Default::default(),
//             legacy_guest_page_size: Default::default(),
//             queue_sel: Default::default(),
//             queue_num_max: Default::default(),
//             queue_num: Default::default(),
//             legacy_queue_align: Default::default(),
//             legacy_queue_pfn: Default::default(),
//             queue_ready: Default::default(),
//             queue_notify: Default::default(),
//             interrupt_status: Default::default(),
//             interrupt_ack: Default::default(),
//             status: Default::default(),
//             // queue_desc_low: Default::default(),
//             // queue_desc_high: Default::default(),
//             queue_desc: Default::default(),
//             // queue_driver_low: Default::default(),
//             // queue_driver_high: Default::default(),
//             queue_driver: Default::default(),
//             // queue_device_low: Default::default(),
//             // queue_device_high: Default::default(),
//             queue_device: Default::default(),
//             config_generation: Default::default(),
//         }
//     }
// }
pub(crate) const LEGACY_VERSION: u32 = 1;
pub(crate) const MODERN_VERSION: u32 = 2;

/// The version of the VirtIO MMIO transport supported by a device.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MmioVersion {
    /// Legacy MMIO transport with page-based addressing.
    Legacy = LEGACY_VERSION,
    /// Modern MMIO transport.
    Modern = MODERN_VERSION,
}

impl TryFrom<u32> for MmioVersion {
    type Error = VirtIoError;

    fn try_from(version: u32) -> Result<Self, Self::Error> {
        match version {
            LEGACY_VERSION => Ok(Self::Legacy),
            MODERN_VERSION => Ok(Self::Modern),
            _ => Err(VirtIoError::MmioError(MmioError::UnsupportedVersion(
                version,
            ))),
        }
    }
}

impl From<MmioVersion> for u32 {
    fn from(version: MmioVersion) -> Self {
        match version {
            MmioVersion::Legacy => LEGACY_VERSION,
            MmioVersion::Modern => MODERN_VERSION,
        }
    }
}

/// MMIO Device Register Interface.
///
/// Ref: 4.2.2 MMIO Device Register Layout and 4.2.4 Legacy interface
#[derive(Debug)]
pub struct MmioTransport {
    header: VirtIOHeader,
    version: MmioVersion,
    io_region: Box<dyn VirtIoDeviceIo>,
}

impl MmioTransport {
    pub fn new(io_region: Box<dyn VirtIoDeviceIo>) -> VirtIoResult<Self> {
        let header = VirtIOHeader::default();
        let magic = header.magic.read(&io_region)?;
        if magic != MAGIC {
            return Err(VirtIoError::MmioError(MmioError::BadMagic(magic)));
        }
        let device_id = header.device_id.read(&io_region)?;
        if device_id == 0 {
            return Err(VirtIoError::MmioError(MmioError::ZeroDeviceId));
        }

        let version = header.version.read(&io_region)?.try_into()?;
        Ok(Self {
            header,
            version,
            io_region,
        })
    }

    /// Gets the version of the VirtIO MMIO transport.
    pub fn version(&self) -> MmioVersion {
        self.version
    }

    /// Gets the vendor ID.
    pub fn vendor_id(&self) -> u32 {
        // Safe because self.header points to a valid VirtIO MMIO region.
        self.header.vendor_id.read(&self.io_region).unwrap()
    }
}

impl Transport for MmioTransport {
    fn device_type(&self) -> VirtIoResult<DeviceType> {
        let ty = self.header.device_id.read(&self.io_region)?;
        Ok(ty.into())
    }

    fn read_device_features(&mut self) -> VirtIoResult<u64> {
        self.header.device_features_sel.write(0, &self.io_region)?;
        let mut device_features = self.header.device_features.read(&self.io_region)? as u64;
        self.header.device_features_sel.write(1, &self.io_region)?;
        device_features |= (self.header.device_features.read(&self.io_region)? as u64) << 32;
        Ok(device_features)
    }

    fn write_driver_features(&mut self, driver_features: u64) -> VirtIoResult<()> {
        self.header.driver_features_sel.write(0, &self.io_region)?;
        self.header
            .driver_features
            .write(driver_features as u32, &self.io_region)?;
        self.header.driver_features_sel.write(1, &self.io_region)?;
        self.header
            .driver_features
            .write((driver_features >> 32) as u32, &self.io_region)
    }

    fn max_queue_size(&mut self, queue: u16) -> VirtIoResult<u32> {
        self.header.queue_sel.write(queue as u32, &self.io_region)?;
        self.header.queue_num_max.read(&self.io_region)
    }

    fn notify(&mut self, queue: u16) -> VirtIoResult<()> {
        self.header
            .queue_notify
            .write(queue as u32, &self.io_region)
    }

    fn get_status(&self) -> VirtIoResult<DeviceStatus> {
        Ok(DeviceStatus::from_bits_truncate(
            self.header.status.read(&self.io_region)?,
        ))
    }

    fn set_status(&mut self, status: DeviceStatus) -> VirtIoResult<()> {
        self.header.status.write(status.bits(), &self.io_region)
    }

    fn set_guest_page_size(&mut self, guest_page_size: u32) -> VirtIoResult<()> {
        match self.version {
            MmioVersion::Legacy => self
                .header
                .legacy_guest_page_size
                .write(guest_page_size, &self.io_region),
            MmioVersion::Modern => Ok(()),
        }
    }

    fn requires_legacy_layout(&self) -> bool {
        self.version == MmioVersion::Legacy
    }

    fn queue_set(
        &mut self,
        queue: u16,
        size: u32,
        descriptors: PhysAddr,
        driver_area: PhysAddr,
        device_area: PhysAddr,
    ) -> VirtIoResult<()> {
        match self.version {
            MmioVersion::Legacy => {
                assert_eq!(
                    driver_area - descriptors,
                    size_of::<Descriptor>() * size as usize
                );
                assert_eq!(
                    device_area - descriptors,
                    align_up(
                        size_of::<Descriptor>() * size as usize
                            + size_of::<u16>() * (size as usize + 3)
                    )
                );
                let align = PAGE_SIZE as u32;
                let pfn = (descriptors / PAGE_SIZE) as u32;
                debug_assert_eq!(pfn as usize * PAGE_SIZE, descriptors);
                self.header.queue_sel.write(queue as _, &self.io_region)?;
                self.header.queue_num.write(size, &self.io_region)?;
                self.header
                    .legacy_queue_align
                    .write(align, &self.io_region)?;
                self.header.legacy_queue_pfn.write(pfn, &self.io_region)
            }
            MmioVersion::Modern => {
                self.header.queue_sel.write(queue as _, &self.io_region)?;
                self.header.queue_num.write(size, &self.io_region)?;
                // self.header
                //     .queue_desc_low
                //     .write(descriptors as _, &self.io_region)?;
                // self.header
                //     .queue_desc_high
                //     .write((descriptors >> 32) as _, &self.io_region)?;
                self.header
                    .queue_desc
                    .write(descriptors as _, &self.io_region)?;
                // self.header
                //     .queue_driver_low
                //     .write(driver_area as _, &self.io_region)?;
                // self.header
                //     .queue_driver_high
                //     .write((driver_area >> 32) as _, &self.io_region)?;
                self.header
                    .queue_driver
                    .write(driver_area as _, &self.io_region)?;
                // self.header
                //     .queue_device_low
                //     .write(device_area as _, &self.io_region)?;
                // self.header
                //     .queue_device_high
                //     .write((device_area >> 32) as _, &self.io_region)?;
                self.header
                    .queue_device
                    .write(device_area as _, &self.io_region)?;
                self.header.queue_ready.write(1, &self.io_region)
            }
        }
    }

    fn queue_unset(&mut self, queue: u16) -> VirtIoResult<()> {
        match self.version {
            MmioVersion::Legacy => {
                self.header.queue_sel.write(queue as _, &self.io_region)?;
                self.header.queue_num.write(0, &self.io_region)?;
                self.header.legacy_queue_align.write(0, &self.io_region)?;
                self.header.legacy_queue_pfn.write(0, &self.io_region)
            }
            MmioVersion::Modern => {
                self.header.queue_sel.write(queue as _, &self.io_region)?;
                self.header.queue_ready.write(0, &self.io_region)?;
                // Wait until we read the same value back, to ensure synchronisation (see 4.2.2.2).
                while self.header.queue_ready.read(&self.io_region)? != 0 {}
                self.header.queue_num.write(0, &self.io_region)?;
                // self.header.queue_desc_low.write(0, &self.io_region)?;
                // self.header.queue_desc_high.write(0, &self.io_region)?;
                self.header.queue_desc.write(0, &self.io_region)?;
                // self.header.queue_driver_low.write(0, &self.io_region)?;
                // self.header.queue_driver_high.write(0, &self.io_region)?;
                self.header.queue_driver.write(0, &self.io_region)?;
                // self.header.queue_device_low.write(0, &self.io_region)?;
                // self.header.queue_device_high.write(0, &self.io_region)
                self.header.queue_device.write(0, &self.io_region)
            }
        }
    }

    fn queue_used(&mut self, queue: u16) -> VirtIoResult<bool> {
        self.header.queue_sel.write(queue as _, &self.io_region)?;
        match self.version {
            MmioVersion::Legacy => Ok(self.header.legacy_queue_pfn.read(&self.io_region)? != 0),
            MmioVersion::Modern => Ok(self.header.queue_ready.read(&self.io_region)? != 0),
        }
    }

    fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        let status = self.header.interrupt_status.read(&self.io_region)?;
        if status == 0 {
            return Ok(false);
        }
        self.header.interrupt_ack.write(status, &self.io_region)?;
        Ok(true)
    }

    fn io_region(&self) -> &dyn VirtIoDeviceIo {
        self.io_region.as_ref()
    }
}

impl Drop for MmioTransport {
    fn drop(&mut self) {
        // Reset the device when the transport is dropped.
        self.set_status(DeviceStatus::empty())
            .expect("failed to reset device")
    }
}
