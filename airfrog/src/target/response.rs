// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Target Response and related types

use alloc::format;
use alloc::string::String;

use airfrog_swd::SwdError;
use airfrog_swd::protocol::{Speed, Version};

use crate::AirfrogError;
use crate::target::Settings;

#[derive(serde::Serialize, Default)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AirfrogError>,
}

impl Response {
    pub fn with_error(mut self, error: AirfrogError) -> Self {
        self.error = Some(error);
        self
    }

    pub fn with_swd_status(mut self, status: Status) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_speed(mut self, speed: Speed) -> Self {
        self.speed = Some(format!("{speed:?}"));
        self
    }
}

impl From<AirfrogError> for Response {
    fn from(error: AirfrogError) -> Self {
        Response::default().with_error(error)
    }
}

impl From<SwdError> for Response {
    fn from(error: SwdError) -> Self {
        Response::default().with_error(error.into())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Status {
    // Whether the target is currently connected.
    pub connected: bool,

    // Version of the SWD protocol used by the target.
    pub version: Option<Version>,

    // IDCODE of the target device, if available.  Note that this is the IDCODE
    // of the SWD interface, not the MCU itself.
    pub idcode: Option<String>,

    // MCU type of the target device, if available.
    pub mcu: Option<String>,

    // Firmware type of the target device, if available.
    pub firmware: Option<String>,

    // Settings for the target.
    pub settings: Settings,
}
