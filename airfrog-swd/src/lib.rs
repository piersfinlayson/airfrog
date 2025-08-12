// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog-swd library
//!
//! ARM Serial Wire Debug (SWD) implementation.
//!
//! This can be used to perform debugging, programming, and co-processing
//! with ARM-based MCUs using the
//! [ARM SWD protocol](https://developer.arm.com/documentation/ihi0031/latest/)
//!
//! It is `no_std` and designed to run on the ESP32, using
//! [embassy](https://embassy.dev/) and
//! [`esp-hal`](https://docs.espressif.com/projects/rust/).  It requires an
//! `alloc` implementation (such as `esp-alloc`).
//!
//! The following diagram shows the key `airfrog-swd` concepts.
//!
//! ```text
//!   airfrog Application  |  bin::Api  ==  WiFi  ==   Programmer/Probe
//! ----------------------                             ----------------
//!     DebugInterface      \                            e.g. probe-rs
//! ----------------------   \
//!      SwdInterface         |--  SwdError
//! ----------------------   /
//!      SwdProtocol        /                          e.g. STM32/Pico
//! ----------------------                            -----------------
//!    ESP32 GPIO pins     >======================<       SWD Target
//!                          3.3V SWDIO/SWCLK/GND
//! ```
//!
//! * [`DebugInterface`] provides the highest-level and most abstracted
//!   interface to perform groups of SWD operations.
//! * [`SwdInterface`] provides a lower-level interface to perform individual
//!   SWD operations.
//! * [`SwdProtocol`] implements the SWD wire protocol through bit-banging.
//!
//! Also included is a server-side binary API implementation [`bin::Api`],
//! which can be used by airfrog firmware to expose SWD function to probes.
//! Those probes should implement the client side of the protocol defined in
//! [`Binary API`](https://github.com/piersfinlayson/airfrog/blob/main/docs/REST-API.md).
//!
//! This is binary server API is exposed over by the default airfrog firmware,
//! and used by probe-rs airfrog probe support.
//!
//! Most applications should use [`DebugInterface`], but those that require
//! tighter control over the target, or are timing sensitive, may want to use
//! [`SwdInterface`] directly.
//!
//! `airfrog-swd` uses and is designed to be used alongside the
//! [`airfrog_core`] library, which provides core debug and hardware concepts
//! used by SWD and the debug interface, but which are not SWD specific.
//!
//! There are a number of
//! [examples](https://github.com/piersfinlayson/airfrog/blob/main/examples/README.md)
//! which show how to use `airfrog-swd` and [`airfrog_core`] to perform various
//! operations, such as connecting to a target, reading memory, erasing flash,
//! and driving peripherals on the target.

#![no_std]

pub mod bin;
pub mod debug;
pub mod interface;
pub mod protocol;

#[doc(inline)]
pub use crate::debug::DebugInterface;
#[doc(inline)]
pub use crate::interface::SwdInterface;
#[doc(inline)]
pub use crate::protocol::SwdProtocol;

extern crate alloc;
use alloc::format;
use alloc::string::String;
use core::fmt;
use serde::Serialize;

/// Core error type used by all airfrog-swd objects
///
/// Methods are provided to make it easier to handle errors, by checking if
/// either a retry or reset is required:
///
/// - [`SwdError::requires_retry()`]
/// - [`SwdError::requires_reset()`]
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwdError {
    /// Transient error that can likely be retried successfully.  When
    /// [`SwdInterface`] is configured to retry on waits, this error is
    /// returned when too many waits have occurred.
    WaitAck,

    /// Represents a fault condition on the target.  This typically means
    /// the target has got into a fault state and needs to be reset using
    /// either [`DebugInterface::initialize_swd_target()`] or a hard reset.
    FaultAck,

    /// Represents no acknowledgement from the target.  This typically means
    /// it got into a bad state and needs to be reset, hopefully via
    /// [`DebugInterface::initialize_swd_target()`], but an external reset
    /// may be required.  The value received is included - but it is unlikely
    /// to be terribly useful. 7 means the SWDIO line was high for the entire
    /// acknowledge cycle, which is the most common case.
    NoAck(u8),

    /// A parity error was detected while reading from the target.  It means
    /// we cannot trust the data read.
    ///
    /// A significant number of parity errors suggest either:
    /// - A noise issue on the SWD lines
    /// - Running the SWD protocol too fast for the SWD lines or target
    ///
    /// To reset, either use [`DebugInterface::initialize_swd_target()`] or
    /// perform a hard reset of the target.
    ReadParity,

    /// A Debug Port error was detected, signalled via the DP CTRL/STAT
    /// register.  This usually requires either writing the ABORT register,
    /// via [`SwdInterface::clear_errors()`], to clear, or resetting the
    /// target.  In reality, a target reset using
    /// [`DebugInterface::initialize_swd_target()`] or a hard reset are likely
    /// to be required.
    DpError,

    /// While there wasn't a SWD protocol level error, the requested option
    /// failed.  Often occurs when a DP/AP register write doesn't "take".
    /// The operation can be retried, but may fail again.  If a target reset
    /// doesn't resolve the issue, it may be a configuration or user error.
    OperationFailed(String),

    /// The target is not ready to receive the requested operation.  This
    /// normally means that the debug domain has not yet been powered up using
    /// [`SwdInterface::power_up_debug_domain()`].  This is done automatically
    /// by [`DebugInterface::initialize_swd_target()`], so if you see this
    /// error, ensure you have called one of those functions first.
    NotReady,

    /// The API was called incorrectly.
    Api,

    /// A network error occurred, such as a timeout or connection failure.
    Network,

    /// A timeout occurred while waiting for a response.
    Timeout,

    /// The requested operation is not supported by `airfrog-swd`.
    Unsupported,
}

impl SwdError {
    fn from_ack(ack: u8) -> Result<(), SwdError> {
        match ack {
            1 => Ok(()),
            2 => Err(SwdError::WaitAck),
            4 => Err(SwdError::FaultAck),
            _ => Err(SwdError::NoAck(ack)),
        }
    }

    /// Returns true if the error requires a target reset to recover.  In this
    /// case issue a new [`DebugInterface::initialize_swd_target()`].  If the error
    /// persists, the target may require a hard reset.
    pub fn requires_reset(&self) -> bool {
        matches!(
            self,
            SwdError::NoAck(_) | SwdError::FaultAck | SwdError::ReadParity | SwdError::DpError
        )
    }

    /// Returns true if the error is a transient error that can be retried.
    /// This is typically just the `Wait` error from the SWD target.
    pub fn requires_retry(&self) -> bool {
        matches!(self, SwdError::WaitAck)
    }

    /// Returns true if the error requires either a reset or retry to recover.
    /// Normally this means an application error - the API has probably been
    /// used incorrectly, or the target is in a bad state.
    pub fn requires_other(&self) -> bool {
        !self.requires_reset() && !self.requires_retry()
    }
}

impl SwdError {
    /// Returns a string representation of the error.
    pub fn as_str(&self) -> &'static str {
        match self {
            SwdError::WaitAck => "Wait ACK",
            SwdError::FaultAck => "Fault ACK",
            SwdError::NoAck(_) => "No ACK",
            SwdError::ReadParity => "Read Parity Error",
            SwdError::DpError => "Debug Port Error",
            SwdError::OperationFailed(_) => "Operation Failed",
            SwdError::NotReady => "Not Ready",
            SwdError::Api => "API Error",
            SwdError::Network => "Network Error",
            SwdError::Timeout => "Timeout",
            SwdError::Unsupported => "Unsupported Operation",
        }
    }
}

impl Serialize for SwdError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SwdError", 2)?;

        let kind = match self {
            SwdError::WaitAck => "wait ack",
            SwdError::FaultAck => "fault ack",
            SwdError::NoAck(_) => "no ack",
            SwdError::ReadParity => "read parity",
            SwdError::DpError => "debug port",
            SwdError::OperationFailed(_) => "operation failed",
            SwdError::NotReady => "not ready",
            SwdError::Api => "api error",
            SwdError::Network => "network error",
            SwdError::Timeout => "timeout",
            SwdError::Unsupported => "unsupported",
        };

        state.serialize_field("kind", kind)?;

        let detail = match self {
            SwdError::OperationFailed(msg) => msg.as_str(),
            SwdError::NoAck(code) => &format!("{code}"),
            _ => "", // empty detail for variants without data
        };
        state.serialize_field("detail", detail)?;
        state.end()
    }
}

impl fmt::Display for SwdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SwdError::NoAck(ack) => write!(f, "{}: {ack}", self.as_str()),
            SwdError::OperationFailed(str) => write!(f, "{}: {str}", self.as_str()),
            _ => write!(f, "{}", self.as_str()),
        }
    }
}

impl From<embedded_io_async::ReadExactError<embassy_net::tcp::Error>> for SwdError {
    fn from(_error: embedded_io_async::ReadExactError<embassy_net::tcp::Error>) -> Self {
        SwdError::Network
    }
}

impl From<embassy_net::tcp::Error> for SwdError {
    fn from(_error: embassy_net::tcp::Error) -> Self {
        SwdError::Network
    }
}

// Macro to handle (binary) API timeouts, where we want the error locally
#[macro_export]
macro_rules! with_timeout_no_return {
    ($timeout:ident, $future:expr) => {
        match embassy_time::with_timeout($timeout, $future).await {
            Ok(result) => Ok(result),
            Err(_) => {
                warn!("Timeout occurred");
                Err(SwdError::Timeout)
            }
        }
    };
}

// Macro to handle (binary) API timeouts where we want an early return
#[macro_export]
macro_rules! with_timeout {
    ($timeout:ident, $future:expr) => {
        match with_timeout_no_return!($timeout, $future) {
            Ok(result) => result,
            Err(e) => {
                return Err(e);
            }
        }
    };
}
