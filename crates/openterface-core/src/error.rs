//! The crate-wide error type.

/// Errors produced anywhere in `openterface-core`.
///
/// Kept coarse-grained and `#[non_exhaustive]` so variants can be added without
/// a breaking change as the protocol/transport layers grow.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A serial transport (open/read/write/baud) operation failed.
    #[error("serial transport error: {0}")]
    Transport(String),

    /// A video capture operation failed.
    #[error("video error: {0}")]
    Video(String),

    /// Device discovery/enumeration failed.
    #[error("device discovery error: {0}")]
    Discovery(String),

    /// A CH9329/HID protocol framing or decoding error.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// A frame could not be decoded to RGBA.
    #[error("decode error: {0}")]
    Decode(String),

    /// Invalid or unsupported configuration.
    #[error("configuration error: {0}")]
    Config(String),

    /// A blocking operation exceeded its deadline.
    #[error("operation timed out")]
    Timeout,

    /// An underlying I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
