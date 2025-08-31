// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Firmware handling library
//!
//! This module contains firmware handling routines for different types of
//! firmware supported by airfrog:
//! - [One ROM](https://piers.rocks/u/one)
//!
//! These routines allow airfrog to load, parse and report information about
//! the firmware on the device airfrog is attached to.

pub(crate) mod one_rom;
pub(crate) mod one_rom_lab;

extern crate alloc;
use alloc::boxed::Box;
use alloc::string::String;
use core::fmt;
use serde_json::Value;
use strum::EnumIter;

use sdrr_fw_parser::Reader;

use crate::http::json::default_formatter;
use airfrog_core::Mcu;

pub(crate) const AF_FW_TYPE_KEY: &str = "_af_fw_type";

#[derive(EnumIter)]
pub enum JsonToHtmlers {
    /// One ROM firmware JSON to HTML formatter
    OneRom(one_rom::JsonToHtmler),

    /// One ROM Lab firmware JSON to HTML formatter
    OneRomLab(one_rom_lab::JsonToHtmler),

    /// Default formatter
    Default(DefaultFormatter), // Is last
}

impl JsonToHtml for JsonToHtmlers {
    fn can_handle(&self, data: &serde_json::Value) -> bool {
        match self {
            JsonToHtmlers::OneRom(handler) => handler.can_handle(data),
            JsonToHtmlers::OneRomLab(handler) => handler.can_handle(data),
            JsonToHtmlers::Default(handler) => handler.can_handle(data),
        }
    }

    fn summary(&self, data: serde_json::Value) -> Result<String, FormatterError> {
        match self {
            JsonToHtmlers::OneRom(handler) => handler.summary(data),
            JsonToHtmlers::OneRomLab(handler) => handler.summary(data),
            JsonToHtmlers::Default(handler) => handler.summary(data),
        }
    }

    fn complete(&self, data: serde_json::Value) -> Result<String, FormatterError> {
        match self {
            JsonToHtmlers::OneRom(handler) => handler.complete(data),
            JsonToHtmlers::OneRomLab(handler) => handler.complete(data),
            JsonToHtmlers::Default(handler) => handler.complete(data),
        }
    }
}

/// Trait for firmware handler information
pub trait FwHandlerInfo {
    /// Return the firmware type name
    fn name() -> &'static str;

    /// Returns whether this handler supports the given MCU
    fn supports_mcu(mcu: &Mcu) -> bool;
}

/// Core trait for firmware handlers that can identify and interact with
/// firmware on ARM targets via SWD
pub trait FwHandler<R: Reader> {
    /// Create a new firmware handler instance
    fn new(reader: R) -> Self;

    /// Do a brief check whether this firmware type is present on the target
    async fn detect(&mut self) -> bool;

    /// Full parsing (assumes detection already passed)
    async fn parse_info(&mut self) -> Result<Box<dyn FwInfo>, FwError<R::Error>>;
}

/// Trait for firmware handlers that can identify and interact with firmware
pub trait FwInfo {
    /// Brief summary for list/overview contexts
    fn summary(&self) -> serde_json::Value;

    /// Full details for dedicated firmware view
    fn details(&self) -> serde_json::Value;
}

// Firmware error type
#[derive(Debug)]
pub enum FwError<E> {
    /// Device not detected
    _NotDetected,

    /// Error reading firmware data
    _Reader(E),

    /// Handler specific error
    #[allow(dead_code)]
    Handler(String),
}

impl<E> From<String> for FwError<E> {
    fn from(s: String) -> Self {
        // Convert string error to your FwError variant
        FwError::Handler(s)
    }
}

/// Formatter error
#[derive(Debug)]
pub enum FormatterError {
    /// Error converting JSON to HTML
    JsonToHtml(String),
}

impl fmt::Display for FormatterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatterError::JsonToHtml(msg) => write!(f, "JSON to HTML conversion error: {msg}"),
        }
    }
}

/// Default formatter for JSON to HTML conversion
#[derive(Debug, Default)]
pub struct DefaultFormatter;
impl JsonToHtml for DefaultFormatter {
    fn can_handle(&self, _data: &serde_json::Value) -> bool {
        true // Default formatter handles all cases
    }

    fn summary(&self, _data: serde_json::Value) -> Result<String, FormatterError> {
        Ok("<p>Unrecognised firmware.</p>".into())
    }

    /// Convert a JSON value to HTML representation
    fn complete(&self, value: Value) -> Result<String, FormatterError> {
        Ok(default_formatter(value))
    }
}

/// Trait for converting JSON data to HTML representation
pub(crate) trait JsonToHtml {
    fn can_handle(&self, data: &serde_json::Value) -> bool;

    fn summary(&self, data: serde_json::Value) -> Result<String, FormatterError>;

    fn complete(&self, data: serde_json::Value) -> Result<String, FormatterError>;
}
