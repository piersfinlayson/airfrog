// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Error types

use core::fmt;
use serde::Serialize;

use airfrog_swd::SwdError;

/// Airfrog default firmware error type
#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum AirfrogError {
    #[serde(rename = "swd")]
    Swd(SwdError),
    #[serde(rename = "airfrog")]
    Airfrog(ErrorKind),
}

impl fmt::Display for AirfrogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AirfrogError::Swd(swd) => write!(f, "{swd}"),
            AirfrogError::Airfrog(kind) => write!(f, "{kind}"),
        }
    }
}

/// AirfrogError::Airfrog error kinds
#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    InvalidBody,
    InvalidPath,
    _InvalidMethod,
    Timeout,
    TooLarge,
    InternalServerError,
    Api,
    Network,
    NoFirmware,
    Flash,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::BadRequest => write!(f, "Bad request"),
            ErrorKind::InvalidBody => write!(f, "Invalid body"),
            ErrorKind::InvalidPath => write!(f, "Invalid path"),
            ErrorKind::_InvalidMethod => write!(f, "Invalid method"),
            ErrorKind::Timeout => write!(f, "Timeout"),
            ErrorKind::TooLarge => write!(f, "Request too large"),
            ErrorKind::InternalServerError => write!(f, "Internal server error"),
            ErrorKind::Api => write!(f, "api error"),
            ErrorKind::Network => write!(f, "network error"),
            ErrorKind::NoFirmware => write!(f, "no firmware"),
            ErrorKind::Flash => write!(f, "flash storage error"),
        }
    }
}

impl Serialize for ErrorKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ErrorKind", 2)?;

        let kind = match self {
            ErrorKind::BadRequest => "bad request",
            ErrorKind::InvalidBody => "invalid body",
            ErrorKind::InvalidPath => "invalid path",
            ErrorKind::_InvalidMethod => "invalid method",
            ErrorKind::Timeout => "timeout",
            ErrorKind::TooLarge => "request too large",
            ErrorKind::InternalServerError => "internal server error",
            ErrorKind::Api => "api error",
            ErrorKind::Network => "network error",
            ErrorKind::NoFirmware => "no firmware",
            ErrorKind::Flash => "flash storage error",
        };

        state.serialize_field("kind", kind)?;
        state.serialize_field("detail", "")?; // add detail logic if needed
        state.end()
    }
}

impl AirfrogError {
    pub fn status_code(&self) -> u16 {
        match self {
            AirfrogError::Swd(swd) => Self::status_code_from_swd_error(swd),
            AirfrogError::Airfrog(kind) => kind.status_code(),
        }
    }

    fn status_code_from_swd_error(swd: &SwdError) -> u16 {
        match swd {
            SwdError::WaitAck | SwdError::Timeout => 408, // Request Timeout
            SwdError::NoAck(_) | SwdError::FaultAck | SwdError::ReadParity | SwdError::DpError => {
                500 // Internal Server Error
            }
            SwdError::OperationFailed(_) => 400, // Bad Request
            SwdError::NotReady | SwdError::Network => 503, // Service Unavailable
            SwdError::Api => 400,                // Bad Request
            SwdError::Unsupported => 501,        // Not Implemented
        }
    }
}

impl ErrorKind {
    pub fn status_code(&self) -> u16 {
        match self {
            ErrorKind::BadRequest => 400,          // Bad Request
            ErrorKind::InvalidBody => 400,         // Bad Request
            ErrorKind::InvalidPath => 404,         // Not Found
            ErrorKind::_InvalidMethod => 405,      // Method Not Allowed
            ErrorKind::Timeout => 408,             // Request Timeout
            ErrorKind::TooLarge => 413,            // Payload Too Large
            ErrorKind::InternalServerError => 500, // Internal Server Error
            ErrorKind::Api => 400,                 // Bad Request
            ErrorKind::Network => 503,             // Service Unavailable
            ErrorKind::NoFirmware => 404,          // Not Found
            ErrorKind::Flash => 500,               // Internal Server Error
        }
    }
}

impl From<SwdError> for AirfrogError {
    fn from(error: SwdError) -> Self {
        match error {
            SwdError::Api => AirfrogError::Airfrog(ErrorKind::Api),
            SwdError::Network => AirfrogError::Airfrog(ErrorKind::Network),
            SwdError::Timeout => AirfrogError::Airfrog(ErrorKind::Timeout),
            _ => AirfrogError::Swd(error),
        }
    }
}

impl From<embassy_net::tcp::Error> for AirfrogError {
    fn from(_error: embassy_net::tcp::Error) -> Self {
        AirfrogError::Airfrog(ErrorKind::Network)
    }
}
