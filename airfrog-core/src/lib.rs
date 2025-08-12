// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! Airfrog is the tiny wireless co-processor for ARM.
//!
//! <https://piers.rocks/u/airfrog>
//!
//! airfrog-core - Core protocol and MCU concepts used by Airfrog.
//!
//! Designed to be used in conjunction with the `airfrog-swd` library for ARM
//! Serial Wire Debug (SWD) operations.
//!
//! This library is `no_std` compatible, and requires an `alloc`
//! implementation.

#![no_std]

pub mod arm;
pub mod rp;
pub mod stm;

extern crate alloc;
use core::fmt;
use core::ops::RangeInclusive;
use static_assertions::const_assert;

use crate::arm::ap::Idr;
use crate::arm::dp::IdCode;

/// Represents a target's microcontroller unit.
///
/// `airfrog-swd` can be used to identify the MCU type using this object.  See
/// [`..::airfrog_swd::debug::DebugInterface::get_mcu()`].
#[derive(Debug, Clone, Copy)]
pub enum Mcu {
    /// An STM32 MCU.
    Stm32(stm::StmDetails),

    /// An RP (Raspberry Pi) MCU.
    Rp(rp::RpDetails),

    /// An unknown MCU, identified by its IDCODE.
    Unknown(IdCode),
}

impl Mcu {
    /// Returns MCU's flash base address if available.
    pub fn flash_base(&self) -> Option<u32> {
        match self {
            Mcu::Stm32(details) => details.flash_base(),
            Mcu::Rp(details) => details.flash_base(),
            Mcu::Unknown(_) => None,
        }
    }

    /// Returns the MCU's RAM base address if available.
    pub fn ram_base(&self) -> Option<u32> {
        match self {
            Mcu::Stm32(details) => details.ram_base(),
            Mcu::Rp(details) => details.ram_base(),
            Mcu::Unknown(_) => None,
        }
    }

    /// Returns the MCU's flash size in bytes if available.
    pub fn flash_size_bytes(&self) -> Option<u32> {
        self.flash_size_kb().map(|size| size * 1024)
    }

    /// Returns the MCU's flash size in KB if available.
    pub fn flash_size_kb(&self) -> Option<u32> {
        match self {
            Mcu::Stm32(details) => details.flash_size_kb().map(|size| size.raw() as u32),
            Mcu::Rp(_) => None,
            Mcu::Unknown(_) => None,
        }
    }

    /// Returns the MCU's RAM size in bytes if available.
    pub fn ram_size_bytes(&self) -> Option<u32> {
        self.ram_size_kb().map(|size| size * 1024)
    }

    /// Returns the MCU's RAM size in KB if available.
    pub fn ram_size_kb(&self) -> Option<u32> {
        match self {
            Mcu::Stm32(details) => details.mcu().line().ram_size_kb(),
            Mcu::Rp(details) => details.line().ram_size_kb(),
            Mcu::Unknown(_) => None,
        }
    }

    /// Returns whether this is an STM32 MCU.
    pub fn is_stm32(&self) -> bool {
        matches!(self, Mcu::Stm32(_))
    }

    /// Returns whether this is an STM32F4 MCU.
    pub fn is_stm32f4(&self) -> bool {
        match self {
            Mcu::Stm32(stm) => stm.is_stm32f4(),
            Mcu::Rp(_) => false,
            Mcu::Unknown(_) => false,
        }
    }

    /// Returns whether this is an RP MCU.
    pub fn is_rp(&self) -> bool {
        matches!(self, Mcu::Rp(_))
    }

    /// Returns the size of the specified flash sector in bytes.
    pub fn get_sector_size_bytes(&self, sector: u8) -> Option<u32> {
        match self {
            Mcu::Stm32(details) => details.get_sector_size_bytes(sector),
            Mcu::Rp(_) => None,
            Mcu::Unknown(_) => None,
        }
    }

    /// Returns the size of the specified flash sector in words.
    pub fn get_sector_size_words(&self, sector: u8) -> Option<u32> {
        self.get_sector_size_bytes(sector).map(|size| size / 4)
    }

    /// Returns the size of the specified flash sector in KB.
    pub fn get_sector_size_kb(&self, sector: u8) -> Option<u32> {
        self.get_sector_size_bytes(sector).map(|size| size / 1024)
    }

    /// Maximum number of flash sectors for STM32 devices.
    pub const MAX_SECTORS: u8 = 12;

    /// Returns the sector number or numbers for the given word range.
    ///
    /// Note that the word range is relative to the start of flash.
    pub fn get_sectors_from_word_range(
        &self,
        range: RangeInclusive<u32>,
        sectors: &mut [u8; Self::MAX_SECTORS as usize],
    ) -> Option<usize> {
        const_assert!(Mcu::MAX_SECTORS <= stm::StmDetails::MAX_SECTORS);
        match self {
            Mcu::Stm32(details) => details.get_sectors_from_word_range(range, sectors),
            Mcu::Rp(_) => None,
            Mcu::Unknown(_) => None,
        }
    }

    /// Returns the expected Access Port Identification Register (IDR) value
    /// for this MCU.
    pub fn expected_idr(&self) -> Option<Idr> {
        match self {
            Mcu::Stm32(details) => details.expected_idr(),
            Mcu::Rp(details) => details.expected_idr(),
            Mcu::Unknown(_) => None,
        }
    }
}

impl fmt::Display for Mcu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mcu::Stm32(details) => {
                if f.alternate() {
                    write!(f, "STM32 MCU: {details:#}")
                } else {
                    write!(f, "{details}")
                }
            }
            Mcu::Rp(details) => {
                if f.alternate() {
                    write!(f, "RP MCU: {details:#}")
                } else {
                    write!(f, "{details}")
                }
            }
            Mcu::Unknown(idcode) => {
                if f.alternate() {
                    write!(f, "Unknown MCU (IDCODE: {idcode:#})")
                } else {
                    write!(f, "Unknown MCU (IDCODE: {idcode})")
                }
            }
        }
    }
}
