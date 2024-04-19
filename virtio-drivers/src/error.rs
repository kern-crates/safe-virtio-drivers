use core::fmt;
use core::fmt::{Display, Formatter};

/// The type returned by driver methods.
pub type VirtIoResult<T> = Result<T, VirtIoError>;

/// The error type of VirtIO drivers.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VirtIoError {
    /// There are not enough descriptors available in the virtqueue, try again later.
    QueueFull,
    /// The device is not ready.
    NotReady,
    /// The device used a different descriptor chain to the one we were expecting.
    WrongToken,
    /// The queue is already in use.
    AlreadyUsed,
    /// Invalid parameter.
    InvalidParam,
    /// Failed to alloc DMA memory.
    DmaError,
    /// I/O Error
    IoError,
    /// The request was not supported by the device.
    Unsupported,
    /// The config space advertised by the device is smaller than the driver expected.
    ConfigSpaceTooSmall,
    /// The device doesn't have any config space, but the driver expects some.
    ConfigSpaceMissing,
    // Error from the socket device.
    // SocketDeviceError(device::socket::SocketError),
}


impl Display for VirtIoError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::QueueFull => write!(f, "Virtqueue is full"),
            Self::NotReady => write!(f, "Device not ready"),
            Self::WrongToken => write!(
                f,
                "Device used a different descriptor chain to the one we were expecting"
            ),
            Self::AlreadyUsed => write!(f, "Virtqueue is already in use"),
            Self::InvalidParam => write!(f, "Invalid parameter"),
            Self::DmaError => write!(f, "Failed to allocate DMA memory"),
            Self::IoError => write!(f, "I/O Error"),
            Self::Unsupported => write!(f, "Request not supported by device"),
            Self::ConfigSpaceTooSmall => write!(
                f,
                "Config space advertised by the device is smaller than expected"
            ),
            Self::ConfigSpaceMissing => {
                write!(
                    f,
                    "The device doesn't have any config space, but the driver expects some"
                )
            }
            // Self::SocketDeviceError(e) => write!(f, "Error from the socket device: {e:?}"),
        }
    }
}



// impl From<device::socket::SocketError> for Error {
//     fn from(e: device::socket::SocketError) -> Self {
//         Self::SocketDeviceError(e)
//     }
// }

