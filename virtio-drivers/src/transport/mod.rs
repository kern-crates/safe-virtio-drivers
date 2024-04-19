use bitflags::bitflags;

pub mod mmio;

bitflags! {
    /// The device status field. Writing 0 into this field resets the device.
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct DeviceStatus: u32 {
        /// Indicates that the guest OS has found the device and recognized it
        /// as a valid virtio device.
        const ACKNOWLEDGE = 1;
        /// Indicates that the guest OS knows how to drive the device.
        const DRIVER = 2;
        /// Indicates that something went wrong in the guest, and it has given
        /// up on the device. This could be an internal error, or the driver
        /// didn’t like the device for some reason, or even a fatal error
        /// during device operation.
        const FAILED = 128;
        /// Indicates that the driver has acknowledged all the features it
        /// understands, and feature negotiation is complete.
        const FEATURES_OK = 8;
        /// Indicates that the driver is set up and ready to drive the device.
        const DRIVER_OK = 4;
        /// Indicates that the device has experienced an error from which it
        /// can’t recover.
        const DEVICE_NEEDS_RESET = 64;
    }
}
