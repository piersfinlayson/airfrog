//! airfrog - Objects used to add support for specific firmware type
//!
//! To add a new firmware type:
//! - Create a FirmwareTypeDecoder, implementing [`Decoder`]
//! - Create a FirmwareTypeFirmware, implementing [`Firmware`]
//! - Add a type to [`FirmwareType`].
//! - Add a `Display` implementation for your [`FirmwareType`]
//! - Add your decoder to [`FirmwareRegistry::DECODERS`].
//! - Add `mod <your_module>;` to [`self`]
//!
//! See [`crate::firmware::onerom`] and [`crate::firmware::onerom_lab`] for
//! example implementations.

// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use async_trait::async_trait;
use embassy_time::{Duration, TimeoutError, with_timeout};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use serde_json::Value;

use airfrog_core::Mcu;
use airfrog_rpc::io::Reader;

use crate::{
    firmware::{Error, FirmwareReader},
    http::{Method, StatusCode},
};

pub const FIRMWARE_TIMEOUT: Duration = Duration::from_millis(2000);

/// Custom firmware types.
///
/// If you are adding a new firmware type, you add its type here, and how you
/// would like it displayed in the `impl Display` following.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareType {
    Unknown,
    OneRom,
    OneRomLab,
}

impl core::fmt::Display for FirmwareType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FirmwareType::Unknown => write!(f, "Unknown"),
            FirmwareType::OneRom => write!(f, "One ROM"),
            FirmwareType::OneRomLab => write!(f, "One ROM Lab"),
        }
    }
}

// The firmware registry, which handles working through each firmware type's
// Decoder to find one which can decide the firmware.
pub(crate) struct FirmwareRegistry;

impl FirmwareRegistry {
    const DECODERS: &'static [&'static dyn Decoder<FirmwareReader>] = &[
        &crate::firmware::onerom::OneRomDecoder::<FirmwareReader>::new(),
        &crate::firmware::onerom_lab::OneRomLabDecoder::<FirmwareReader>::new(),
    ];

    // Go through each firmware type's Decoder in turn seeing if it can decode
    // this firmware.  If so, decode it and return the decoded firmware object.
    // This is a bit involved as both detect() and decode() are async, so we
    // need to run with timeouts.
    pub async fn detect_and_decode(
        mcu: &Mcu,
        reader: &mut FirmwareReader,
    ) -> Option<Box<dyn Firmware<FirmwareReader>>> {
        for decoder in Self::DECODERS {
            match with_timeout(FIRMWARE_TIMEOUT, decoder.detect(mcu, reader)).await {
                Err(TimeoutError) => {
                    warn!(
                        "Timeout during firmware detection for {}",
                        decoder.fw_type()
                    );
                    continue;
                }
                Ok(Some(fw_type)) => {
                    info!("Detected firmware: {fw_type}");
                    match with_timeout(FIRMWARE_TIMEOUT, decoder.decode(mcu, reader)).await {
                        Ok(Ok(firmware)) => {
                            info!("Successfully decoded firmware: {fw_type}");
                            return Some(firmware);
                        }
                        Ok(Err(e)) => {
                            warn!("Decoder {fw_type} failed to decode firmware {e:?}");
                        }
                        Err(TimeoutError) => {
                            warn!("Timeout during firmware decoding for {fw_type}");
                        }
                    }
                }
                Ok(None) => {
                    trace!("Decoder {} did not detect firmware", decoder.fw_type());
                    continue;
                }
            }
        }
        None
    }
}

/// Each firmware type implementation must provide a factory, to detect and
/// and decode the firmware type it supports.
///
/// Use #[async_trait(?Send)] in front of the trait implementation or the
/// compiler will return cryptic errors.
///
/// Async methods taking longer than [`FIRMWARE_TIMEOUT`] to run will be
/// cancelled.
#[async_trait(?Send)]
pub trait Decoder<R: Reader> {
    /// Returns the factory's supported firmware type.  A single factory can
    /// only support a single firmware type.
    fn fw_type(&self) -> FirmwareType;

    /// Performs a simple detection for this firmware type.  Should not
    /// perform a full decode - just detects if the MCU's firmware is the
    /// factory's type.
    ///
    /// This is typically done by checking:
    /// - whether the MCU is supported by this firmware
    /// - if so, looking for magic value(s) in flash and/or RAM.
    async fn detect(&self, mcu: &Mcu, reader: &mut R) -> Option<FirmwareType>;

    /// Performs a full decode/analyse of this firmware.
    ///
    /// When implementing, ensure that enough information is retrieved and
    /// stored in the [`Firmware<R>`] object to be able to handle the
    /// [`Firmware`] trait methods that do not permit [`Reader`] use.
    async fn decode(&self, mcu: &Mcu, reader: &mut R) -> Result<Box<dyn Firmware<R>>, Error>;
}

/// Trait for implementing custom Firmware type plugins for Airfrog.
///
/// Whereas [`Decoder`] is used to detect and decode a particular type of
/// firmware, this trait is used to perform actions on that firmware - both on
/// the decoded firmware object returned by [`Decoder::decode`], and additional
/// actions that require interaction with the target.
///
/// Use #[async_trait(?Send)] in front of the trait implementation or the
/// compiler will return cryptic errors.
///
/// Async methods taking longer than [`FIRMWARE_TIMEOUT`] to run will be
/// cancelled.
#[async_trait(?Send)]
pub trait Firmware<R: Reader> {
    /// Returns this firmware type.
    ///
    /// Reading from the Target is not permitted in this method.
    fn fw_type(&self) -> FirmwareType;

    /// Returns location of the SEGGER_RTT control block for this firmware,
    /// i.e. a RAM address.
    ///
    /// Reading from the Target is not permitted in this method.
    fn rtt_cb_address(&self) -> Option<u32>;

    /// Return a Vec of key value pairs listing key properties of this
    /// firmware, for inclusion in a firmware summary web page.  As this will
    /// be shown at the start of a number of Airfrog's pages, keep the number
    /// of entries low - under 10.  Http may decide to truncate this list.
    /// Items will be displayed in the order they are in the Vec.
    ///
    /// Reading from the Target is not permitted in this method.
    fn get_summary_kvp(&self) -> Result<Vec<(String, String)>, Error>;

    /// Return the full firmware properties as a Vec of sections of HTML.  Each
    /// section will be automatically placed within a <div class="card"></div>,
    /// and all sections will be below an existing <h1></h1> heading.  Return
    /// at least one section.
    ///
    /// Reading from the Target is not permitted in this method.
    fn get_full_html(&self) -> Result<(StatusCode, Option<String>), Error>;

    /// Return a Vec of [`WwwButton`] instances consisting of button name
    /// and HTTP path.
    ///
    /// Airfrog will take these pairs and add additional buttons to the
    /// firmware footer in the Airfrog footer for your firmware, with a button
    /// press performing a GET for /www/firmware/path.  Keep the number of
    /// buttons low (1-3).  Httpd may truncate if too many are returned.
    ///
    /// It is not necessary to return values here in order for Airfrog to pass
    /// REST or WWW requests for URLs below /api/firmware and /www/firmware via
    /// `handle_rest()` and `handle_www()`, but this makes any key www URLs
    /// discoverable, and linking out/using other URLs within those pages.
    ///
    /// Reading from the Target is not permitted in this method.
    fn get_buttons(&self) -> Result<Vec<WwwButton>, Error>;

    /// Handle a REST request, which was received at /api/firmware/path.
    /// This allows the firmware type to implement custom REST APIs.  The
    /// response must include a HTTP StatusCode and optional serde::Value to be
    /// converted into JSON.
    ///
    /// This method may read from the target.
    async fn handle_rest(
        &self,
        method: Method,
        path: String,
        body: Option<Value>,
        reader: &mut R,
    ) -> Result<(StatusCode, Option<Value>), Error>;

    /// Handle a WWW request, which was received at /www/firmware/path.  This
    /// allows the firmware type to implement a custom WWW interface within
    /// Airfrog.
    ///
    /// This method returns the HTML content for the page, which will be
    /// wrapped by Airfrog's standard <head></head> and toolbar footer.
    ///
    /// This method may read from the target.
    async fn handle_www(
        &self,
        method: Method,
        path: String,
        body: Option<String>,
        reader: &mut R,
    ) -> Result<(StatusCode, Option<String>), Error>;
}

/// A custom button to display in the Airfrog's Firmware UI footer, for this
/// firmware type.
#[derive(Debug, PartialEq, Eq, Clone, serde::Serialize, serde::Deserialize)]
pub struct WwwButton {
    /// The name to appear on the button
    pub name: String,

    /// The HTTP path for the button to GET (below /www/firmware/).  Do not
    /// include the leading /.
    pub path: String,
}
