// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog-core - Raspberry Pi (RPxxxx) specific objects

use core::fmt;

use crate::arm::Cortex;
use crate::arm::ap::{IDR_AHB_AP_CORTEX_M0, IDR_AHB_AP_CORTEX_M33, Idr};
use crate::arm::dp::IdCode;

// RP2040 Flash base addresses
const RP2040_FLASH_BASE: u32 = 0x1000_0000;

// RP2040 RAM base addresses
const RP2040_RAM_BASE: u32 = 0x2000_0000;

// RP2350 Flash base addresses
const RP2350_FLASH_BASE: u32 = 0x1000_0000;

// RP2350 RAM base addresses
const RP2350_RAM_BASE: u32 = 0x2000_0000;

// RP2040 chip ID
pub const RP2040_CHIP_ID: u32 = 0x10002927;
pub const RP2040_CHIP_ID_ADDR: u32 = 0x40000000;

// RP2040 CPU ID
pub const RP2040_CPU_ID: u32 = 0x410CC601;
pub const RP2040_CPU_ID_ADDR: u32 = 0xE000ED00;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpLine {
    Rp2040,
    Rp2350,
    Unknown,
}

impl RpLine {
    /// Whether this `airfrog-core` is familiar with this RP family
    pub fn known(&self) -> bool {
        !matches!(self, RpLine::Unknown)
    }

    /// Returns the RAM size in bytes
    pub fn ram_size_bytes(&self) -> Option<u32> {
        self.ram_size_kb().map(|size| size * 1024)
    }

    /// Returns the RAM size in KB
    pub fn ram_size_kb(&self) -> Option<u32> {
        match self {
            RpLine::Rp2040 => Some(264),
            RpLine::Rp2350 => Some(512),
            RpLine::Unknown => None,
        }
    }

    /// Returns the expected IDR value for this RP line.
    pub fn expected_idr(&self) -> Option<Idr> {
        match self {
            RpLine::Rp2040 => Some(IDR_AHB_AP_CORTEX_M0),
            RpLine::Rp2350 => Some(IDR_AHB_AP_CORTEX_M33),
            RpLine::Unknown => None,
        }
    }
}

impl fmt::Display for RpLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpLine::Rp2040 => write!(f, "RP2040"),
            RpLine::Rp2350 => write!(f, "RP2350"),
            RpLine::Unknown => write!(f, "Unknown"),
        }
    }
}

/// RP device details
#[derive(Debug, Clone, Copy)]
pub struct RpDetails {
    line: RpLine,
}

impl RpDetails {
    /// Create a new `RpDetails` instance from a specific RP line
    pub fn from_line(line: RpLine) -> Self {
        Self { line }
    }

    /// Get the family of this RP device
    pub fn line(&self) -> RpLine {
        self.line
    }

    /// Create a new `RpDetails` instance from IdCode
    pub fn from_idcode(idcode: IdCode) -> Option<Self> {
        match idcode {
            Cortex::IDCODE_M0 => Some(RpDetails {
                line: RpLine::Rp2040,
            }),
            Cortex::IDCODE_M33 => Some(RpDetails {
                line: RpLine::Rp2350,
            }),
            _ => None,
        }
    }

    /// Get the base flash address for this MCU
    pub fn flash_base(&self) -> Option<u32> {
        match self.line {
            RpLine::Rp2040 => Some(RP2040_FLASH_BASE),
            RpLine::Rp2350 => Some(RP2350_FLASH_BASE),
            RpLine::Unknown => None,
        }
    }

    /// Get the base RAM address for this MCU
    pub fn ram_base(&self) -> Option<u32> {
        match self.line {
            RpLine::Rp2040 => Some(RP2040_RAM_BASE),
            RpLine::Rp2350 => Some(RP2350_RAM_BASE),
            RpLine::Unknown => None,
        }
    }

    /// Returns the expected IDR value for this device.
    ///
    /// Returns:
    /// - `Some(Idr)`: If the RP line is known.
    /// - `None`: If the RP line is unknown.
    pub fn expected_idr(&self) -> Option<Idr> {
        self.line.expected_idr()
    }
}

impl fmt::Display for RpDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.line)
    }
}
