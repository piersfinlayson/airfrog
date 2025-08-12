// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog-core - STM32 specific objects

use alloc::{format, string::String};
use core::fmt;
use core::ops::RangeInclusive;

use crate::arm::Cortex;
use crate::arm::ap::Idr;
use crate::arm::ap::{IDR_AHB_AP_CORTEX_M3, IDR_AHB_AP_CORTEX_M4};
use crate::arm::dp::IdCode;

// STM32F4 Flash memory base address
const STM32F1_FLASH_BASE: u32 = 0x0800_0000;
const STM32F4_FLASH_BASE: u32 = 0x0800_0000;

// STM32F4 RAM memory base address
const STM32F4_RAM_BASE: u32 = 0x2000_0000;
const STM32F1_RAM_BASE: u32 = 0x2000_0000;

// GPIO register offsets
const GPIOX_MODER_OFFSET: u32 = 0x00;
const GPIOX_OTYPER_OFFSET: u32 = 0x04;
const GPIOX_OSPEEDR_OFFSET: u32 = 0x08;
const GPIOX_PUPDR_OFFSET: u32 = 0x0C;
const GPIOX_IDR_OFFSET: u32 = 0x10;
const GPIOX_ODR_OFFSET: u32 = 0x14;
const GPIOX_BSRR_OFFSET: u32 = 0x18;
const GPIOX_LCKR_OFFSET: u32 = 0x1C;
const GPIOX_AFRL_OFFSET: u32 = 0x20;
const GPIOX_AFRH_OFFSET: u32 = 0x24;

/// STM32F4 GPIO Port A register addresses
const STM32F4_GPIOA_REG_BASE: u32 = 0x4002_0000;
pub const STM32F4_GPIOA_MODER: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_MODER_OFFSET;
pub const STM32F4_GPIOA_OTYPER: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_OTYPER_OFFSET;
pub const STM32F4_GPIOA_OSPEEDR: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_OSPEEDR_OFFSET;
pub const STM32F4_GPIOA_PUPDR: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_PUPDR_OFFSET;
pub const STM32F4_GPIOA_IDR: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_IDR_OFFSET;
pub const STM32F4_GPIOA_ODR: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_ODR_OFFSET;
pub const STM32F4_GPIOA_BSRR: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_BSRR_OFFSET;
pub const STM32F4_GPIOA_LCKR: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_LCKR_OFFSET;
pub const STM32F4_GPIOA_AFRL: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_AFRL_OFFSET;
pub const STM32F4_GPIOA_AFRH: u32 = STM32F4_GPIOA_REG_BASE + GPIOX_AFRH_OFFSET;

/// STM32F4 GPIO Port B register base address
const STM32F4_GPIOB_REG_BASE: u32 = 0x4002_0400;
pub const STM32F4_GPIOB_MODER: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_MODER_OFFSET;
pub const STM32F4_GPIOB_OTYPER: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_OTYPER_OFFSET;
pub const STM32F4_GPIOB_OSPEEDR: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_OSPEEDR_OFFSET;
pub const STM32F4_GPIOB_PUPDR: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_PUPDR_OFFSET;
pub const STM32F4_GPIOB_IDR: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_IDR_OFFSET;
pub const STM32F4_GPIOB_ODR: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_ODR_OFFSET;
pub const STM32F4_GPIOB_BSRR: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_BSRR_OFFSET;
pub const STM32F4_GPIOB_LCKR: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_LCKR_OFFSET;
pub const STM32F4_GPIOB_AFRL: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_AFRL_OFFSET;
pub const STM32F4_GPIOB_AFRH: u32 = STM32F4_GPIOB_REG_BASE + GPIOX_AFRH_OFFSET;

/// STM32F4 GPIO Port C register base address
const STM32F4_GPIOC_REG_BASE: u32 = 0x4002_0800;
pub const STM32F4_GPIOC_MODER: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_MODER_OFFSET;
pub const STM32F4_GPIOC_OTYPER: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_OTYPER_OFFSET;
pub const STM32F4_GPIOC_OSPEEDR: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_OSPEEDR_OFFSET;
pub const STM32F4_GPIOC_PUPDR: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_PUPDR_OFFSET;
pub const STM32F4_GPIOC_IDR: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_IDR_OFFSET;
pub const STM32F4_GPIOC_ODR: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_ODR_OFFSET;
pub const STM32F4_GPIOC_BSRR: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_BSRR_OFFSET;
pub const STM32F4_GPIOC_LCKR: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_LCKR_OFFSET;
pub const STM32F4_GPIOC_AFRL: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_AFRL_OFFSET;
pub const STM32F4_GPIOC_AFRH: u32 = STM32F4_GPIOC_REG_BASE + GPIOX_AFRH_OFFSET;

/// STM32F4 GPIO Port D register base address
const STM32F4_GPIOD_REG_BASE: u32 = 0x4002_0C00;
pub const STM32F4_GPIOD_MODER: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_MODER_OFFSET;
pub const STM32F4_GPIOD_OTYPER: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_OTYPER_OFFSET;
pub const STM32F4_GPIOD_OSPEEDR: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_OSPEEDR_OFFSET;
pub const STM32F4_GPIOD_PUPDR: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_PUPDR_OFFSET;
pub const STM32F4_GPIOD_IDR: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_IDR_OFFSET;
pub const STM32F4_GPIOD_ODR: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_ODR_OFFSET;
pub const STM32F4_GPIOD_BSRR: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_BSRR_OFFSET;
pub const STM32F4_GPIOD_LCKR: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_LCKR_OFFSET;
pub const STM32F4_GPIOD_AFRL: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_AFRL_OFFSET;
pub const STM32F4_GPIOD_AFRH: u32 = STM32F4_GPIOD_REG_BASE + GPIOX_AFRH_OFFSET;

/// STM32F4 GPIO Port E register addresses
const STM32F4_GPIOE_REG_BASE: u32 = 0x4002_1000;
pub const STM32F4_GPIOE_MODER: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_MODER_OFFSET;
pub const STM32F4_GPIOE_OTYPER: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_OTYPER_OFFSET;
pub const STM32F4_GPIOE_OSPEEDR: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_OSPEEDR_OFFSET;
pub const STM32F4_GPIOE_PUPDR: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_PUPDR_OFFSET;
pub const STM32F4_GPIOE_IDR: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_IDR_OFFSET;
pub const STM32F4_GPIOE_ODR: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_ODR_OFFSET;
pub const STM32F4_GPIOE_BSRR: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_BSRR_OFFSET;
pub const STM32F4_GPIOE_LCKR: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_LCKR_OFFSET;
pub const STM32F4_GPIOE_AFRL: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_AFRL_OFFSET;
pub const STM32F4_GPIOE_AFRH: u32 = STM32F4_GPIOE_REG_BASE + GPIOX_AFRH_OFFSET;

/// STM32F4 GPIO MODER values
pub const STM32F4_MODER_OUTPUT: u32 = 0b01;
pub const STM32F4_MODER_INPUT: u32 = 0b00;
pub const STM32F4_MODER_AF: u32 = 0b10;
pub const STM32F4_MODER_ANALOG: u32 = 0b11;
pub const STM32F4_MODER_MASK: u32 = 0b11;

/// STM32F4 GPIO PUPDR values
pub const STM32F4_PUPDR_NONE: u32 = 0b00;
pub const STM32F4_PUPDR_PU: u32 = 0b01;
pub const STM32F4_PUPDR_PD: u32 = 0b10;
pub const STM32F4_PUPDR_MASK: u32 = 0b11;

// STM32F4 FLASH register base address
const STM32F4_FLASH_REG_BASE: u32 = 0x4002_3C00;

/// STM32F4 FLASH_CR register
///
/// Used to control flash erasing and programming operations.
pub struct Stm32F4FlashCr;

impl Stm32F4FlashCr {
    /// STM32F4 memory address of this register
    pub const ADDRESS: u32 = STM32F4_FLASH_REG_BASE + 0x10;

    /// STM32F4 FLASH_CR register bit positions
    pub const LOCK_BIT: u32 = 31;
    pub const ERRIE_BIT: u32 = 25;
    pub const EOPIE_BIT: u32 = 24;
    pub const STRT_BIT: u32 = 16;
    pub const MER_BIT: u32 = 2;
    pub const SER_BIT: u32 = 1;
    pub const PG_BIT: u32 = 0;

    /// STM32F4 FLASH_CR register shift values
    pub const SNB_SHIFT: u32 = 3;
    pub const PSIZE_SHIFT: u32 = 8;

    /// STM32F4 FLASH_CR register masks
    pub const SNB_MASK: u32 = 0b1111;
    pub const PSIZE_MASK: u32 = 0b11;

    /// STM32F4 FLASH_CR PSIZE values.  `PSIZE_X64` is the fastest programming
    /// size, but requires the highest VCC (3.3V is fine).
    pub const PSIZE_X8: u32 = 0b00;
    pub const PSIZE_X16: u32 = 0b01;
    pub const PSIZE_X32: u32 = 0b10;
    pub const PSIZE_X64: u32 = 0b11;
}

/// STM32F4 FLASH_SR register
///
/// Used to check the status of flash operations, including errors and busy
/// state.
pub struct Stm32F4FlashSr(u32);

impl Stm32F4FlashSr {
    /// STM32F4 memory address of this register
    pub const ADDRESS: u32 = STM32F4_FLASH_REG_BASE + 0x0C;

    /// STM32F4 FLASH_SR register bit positions
    pub const EOP_BIT: u32 = 0;
    pub const OPERR_BIT: u32 = 1;
    pub const WRPERR_BIT: u32 = 4;
    pub const PGAERR_BIT: u32 = 5;
    pub const PGPERR_BIT: u32 = 6;
    pub const PGSERR_BIT: u32 = 7;
    pub const RDERR_BIT: u32 = 8;
    pub const BSY_BIT: u32 = 16;

    /// Whether a flash operation is in progress.
    pub fn busy(&self) -> bool {
        (self.0 >> Self::BSY_BIT) & 1 != 0
    }

    /// Whether there are any errors in the flash status register.
    pub fn errors(&self) -> bool {
        let error_mask = (1 << Self::OPERR_BIT)
            | (1 << Self::WRPERR_BIT)
            | (1 << Self::PGAERR_BIT)
            | (1 << Self::PGPERR_BIT)
            | (1 << Self::PGSERR_BIT)
            | (1 << Self::RDERR_BIT);
        self.0 & error_mask != 0
    }
}

impl From<u32> for Stm32F4FlashSr {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Stm32F4FlashSr> for u32 {
    fn from(sr: Stm32F4FlashSr) -> Self {
        sr.0
    }
}

impl fmt::Display for Stm32F4FlashSr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}", self.0)
    }
}

/// STM32F4 FLASH_KEYR register
///
/// Used to unlock the flash memory for programming and erasing operations.
pub struct Stm32F4FlashKeyr;

impl Stm32F4FlashKeyr {
    /// STM32F4 memory address of this register
    pub const ADDRESS: u32 = STM32F4_FLASH_REG_BASE + 0x04;

    /// STM32F4 FLASH_KEYR keys used to unlock the flash memory
    pub const KEY1: u32 = 0x45670123;
    pub const KEY2: u32 = 0xCDEF89AB;
}

/// STM32 product family
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StmFamily {
    /// STM32F4 family
    F4,

    /// STM32F1 family
    F1,

    /// Unknown STM32 family
    Unknown,
}

impl StmFamily {
    /// Whether this `airfrog-core` is familiar with this STM32 family
    pub fn known(&self) -> bool {
        !matches!(self, StmFamily::Unknown)
    }
}

impl fmt::Display for StmFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StmFamily::F4 => write!(f, "STM32F4"),
            StmFamily::F1 => write!(f, "STM32F1"),
            StmFamily::Unknown => write!(f, "Unknown"),
        }
    }
}

/// STM32 product line
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StmLine {
    /// STM32F401B/C
    F401BC,

    /// STM32F401D/E
    F401DE,

    /// STM32F411
    F411,

    /// STM32F427/437
    F427_F437,

    /// STM32F413/423
    F413_F423,

    /// STM32F405/415/07/17
    F4x5,

    /// STM32F446
    F446,

    /// STM32F103
    F103,

    /// Unknown STM32 line
    Unknown,
}

impl StmLine {
    /// Returns the STM32's RAM size in bytes if available.
    ///
    /// Only main SRAM is reported.  CCM RAM, if present, is not included.
    /// Use [`Self::ccm_ram_size_bytes`] to get the CCM RAM size.
    pub fn ram_size_bytes(&self) -> Option<u32> {
        self.ram_size_kb().map(|size| size * 1024)
    }

    /// Returns the STM32's RAM size in KB if available.
    ///
    /// Only main SRAM is reported.  CCM RAM, if present, is not included.
    /// Use [`Self::ccm_ram_size_kb`] to get the CCM RAM size
    pub fn ram_size_kb(&self) -> Option<u32> {
        match self {
            StmLine::F401BC => Some(96),
            StmLine::F401DE => Some(96),
            StmLine::F411 => Some(128),
            StmLine::F427_F437 => Some(192),
            StmLine::F413_F423 => Some(256),
            StmLine::F4x5 => Some(128),
            StmLine::F446 => Some(128),
            StmLine::F103 => Some(20),
            StmLine::Unknown => None,
        }
    }

    /// Returns the STM32's CCM RAM size in bytes if available.
    pub fn ccm_ram_size_bytes(&self) -> Option<u32> {
        self.ccm_ram_size_kb().map(|size| size * 1024)
    }

    /// Returns the STM32's CCM RAM size in KB if available.
    pub fn ccm_ram_size_kb(&self) -> Option<u32> {
        match self {
            StmLine::F401BC => None,
            StmLine::F401DE => None,
            StmLine::F411 => None,
            StmLine::F427_F437 => None,
            StmLine::F413_F423 => None,
            StmLine::F4x5 => Some(64),
            StmLine::F446 => None,
            StmLine::F103 => None,
            StmLine::Unknown => None,
        }
    }
}

impl fmt::Display for StmLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StmLine::F401BC => write!(f, "STM32F401B/C"),
            StmLine::F401DE => write!(f, "STM32F401D/E"),
            StmLine::F4x5 => write!(f, "STM32F405/415/07/17"),
            StmLine::F411 => write!(f, "STM32F411"),
            StmLine::F413_F423 => write!(f, "STM32F413/423"),
            StmLine::F427_F437 => write!(f, "STM32F427/437"),
            StmLine::F446 => write!(f, "STM32F446"),
            StmLine::F103 => write!(f, "STM32F103"),
            StmLine::Unknown => write!(f, "Unknown STM32"),
        }
    }
}

/// STM32 MCU Device ID Code
///
/// Device ID format:
/// - Bits 31:16: REV_ID (silicon revision)
/// - Bits 15:0:  DEV_ID (device identifier)
///
/// # Examples
///
/// ```
/// let device_id: StmDeviceId = 0x10006413.into();
/// println!("{}", device_id);
///
/// // Or using new()
/// let device_id = StmDeviceId::new(0x10006413);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StmDeviceId {
    raw: u32,
}

impl StmDeviceId {
    /// STM32F4 memory address of the ROM location holding the device ID
    pub const ADDRESS: u32 = 0xE004_2000;

    /// Create new Device ID decoder from raw 32-bit value
    pub fn new(raw: u32) -> Self {
        Self { raw }
    }

    /// Get raw Device ID value
    pub fn raw(&self) -> u32 {
        self.raw
    }

    /// Get revision field (bits 31:16)
    pub fn revision(&self) -> u16 {
        ((self.raw >> 16) & 0xFFFF) as u16
    }

    /// Get device identifier (bits 15:0)
    pub fn device_id(&self) -> u16 {
        (self.raw & 0xFFF) as u16
    }

    /// Get STM32 product family
    pub fn family(&self) -> StmFamily {
        match self.device_id() {
            0x423 | 0x433 | 0x431 | 0x413 | 0x419 | 0x463 | 0x421 => StmFamily::F4,
            0x412 | 0x410 | 0x414 | 0x430 | 0x418 => StmFamily::F1,
            _ => StmFamily::Unknown,
        }
    }

    /// Get STM32 product line
    pub fn line(&self) -> StmLine {
        match self.device_id() {
            0x423 => StmLine::F401BC,
            0x433 => StmLine::F401DE,
            0x431 => StmLine::F411,
            0x413 => StmLine::F4x5,
            0x419 => StmLine::F427_F437,
            0x463 => StmLine::F413_F423,
            0x421 => StmLine::F446,
            0x410 => StmLine::F103,
            _ => StmLine::Unknown,
        }
    }

    /// Get revision letter if known.  These values are usually documented in
    /// errata sheets or reference manuals for the product line.
    pub fn revision_str(&self) -> &'static str {
        match self.line() {
            StmLine::F401BC | StmLine::F401DE => match self.revision() {
                0x1000 => "A",
                0x1001 => "1/Z",
                _ => "unknown",
            },
            StmLine::F411 => match self.revision() {
                0x1000 => "A/1/Z",
                _ => "unknown",
            },
            StmLine::F4x5 => match self.revision() {
                // Same for F407/417
                0x1000 => "A",
                0x1001 => "Z",
                0x1003 => "1",
                0x1007 => "2",
                0x100F => "Y/4",
                0x101F => "5/6",
                _ => "unknown",
            },
            StmLine::F427_F437 => match self.revision() {
                0x1000 => "A",
                0x1003 => "Y",
                0x2001 => "3",
                0x2003 => "4/5/B",
                _ => "unknown",
            },
            StmLine::F413_F423 => match self.revision() {
                0x1000 => "A/1",
                _ => "unknown",
            },
            StmLine::F446 => match self.revision() {
                0x1000 => "A/1",
                _ => "unknown",
            },
            StmLine::F103 => match self.revision() {
                0x0000 => "A",
                0x2000 => "B",
                0x2001 => "Z",
                0x2003 => "1/2/3/X/Y",
                _ => "unknown",
            },
            StmLine::Unknown => "unknown",
        }
    }

    /// Checks if this is an STM32 device `airfrog-core` is familiar with
    pub fn is_known_device(&self) -> bool {
        !matches!(self.line(), StmLine::Unknown)
    }
}

impl From<u32> for StmDeviceId {
    fn from(raw: u32) -> Self {
        Self::new(raw)
    }
}

impl fmt::Display for StmDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(
                f,
                "STM32 Device ID: 0x{:08X} ({} {})",
                self.raw(),
                self.line(),
                self.revision_str()
            )
        } else {
            write!(f, "{}", self.line())
        }
    }
}

impl fmt::LowerHex for StmDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.raw)
    }
}

impl fmt::UpperHex for StmDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}", self.raw)
    }
}

/// STM32 Unique Device ID (UID)
///
/// This is a 96-bit unique identifier for each STM32 device, consisting of
/// three 32-bit words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StmUniqueId {
    uid: [u32; 3],
}

impl StmUniqueId {
    pub const STM32F4_INITIAL_ADDRESS: u32 = 0x1FFF_7A10;
    pub const STM32F1_INITIAL_ADDRESS: u32 = 0x1FFF_F7E8;

    /// Get the initial address for reading the Unique ID based on the STM32#
    /// family.
    pub fn addr_from_family(family: StmFamily) -> Option<u32> {
        match family {
            StmFamily::F4 => Some(Self::STM32F4_INITIAL_ADDRESS),
            StmFamily::F1 => Some(Self::STM32F1_INITIAL_ADDRESS),
            StmFamily::Unknown => None,
        }
    }

    /// Create new Unique ID from 3 32-bit words.  These are expected to be
    /// provided LSB (UID31:0) first, read from the `INITIAL_ADDRESS`.
    pub fn new(uid: [u32; 3]) -> Self {
        Self { uid }
    }

    /// UID31:0 - X/Y co-ordinates on the wafer.  X co-ordinate is the lower
    /// 16 bits, Y co-ordinate is the upper 16 bits.
    // https://community.st.com/t5/stm32-mcus-products/parsing-uid-fields-on-stm32l476-wafer-x-y-coordinates-and-bcd/td-p/820338
    pub fn x_y(&self) -> u32 {
        self.uid[0]
    }

    /// UID15:0 X-coordinate on the wafer
    pub fn x(&self) -> u16 {
        (self.x_y() & 0xFFFF) as u16
    }

    /// UID31:16 Y-coordinate on the wafer
    pub fn y(&self) -> u16 {
        ((self.x_y() >> 16) & 0xFFFF) as u16
    }

    /// UID39:32 - Wafer number (reference manual claims this is ASCII encoded
    /// but isn't - it is an 8-bit value).
    pub fn wafer(&self) -> u8 {
        (self.uid[1] & 0xFF) as u8
    }

    /// UID63:40 - Lot23:0, ASCII encoded
    pub fn lot(&self) -> [u8; 7] {
        let uid1_bytes = self.uid[1].to_le_bytes();
        let uid2_bytes = self.uid[2].to_le_bytes();

        [
            uid2_bytes[3], // byte 11
            uid2_bytes[2], // byte 10
            uid2_bytes[1], // byte 9
            uid2_bytes[0], // byte 8
            uid1_bytes[3], // byte 7
            uid1_bytes[2], // byte 6
            uid1_bytes[1], // byte 5
        ]
    }

    /// Wafer number as string
    pub fn wafer_str(&self) -> String {
        let byte = self.wafer();
        format!("0x{byte:02X}")
    }

    /// Lot number as string  
    pub fn lot_str(&self) -> String {
        let bytes = self.lot();

        let mut result = String::new();
        for &byte in &bytes {
            if byte.is_ascii_graphic() {
                result.push(byte as char);
            } else {
                result.push('.')
            }
        }
        result
    }

    /// Get the raw UID as an array of 3 u32 values, UID31:0 first
    pub fn raw(&self) -> [u32; 3] {
        self.uid
    }
}

impl fmt::Display for StmUniqueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UID[95:0]: 0x{:08X}{:08X}{:08X} Lot: {} Wafer: {} X/Y: {}/{}",
            self.raw()[2],
            self.raw()[1],
            self.raw()[0],
            self.lot_str(),
            self.wafer_str(),
            self.x(),
            self.y(),
        )
    }
}

/// STM Flash Size Register
///
/// This is a 16-bit value that indicates the size of the flash memory in
/// kilobytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StmFlashSize {
    raw: u16,
}

impl StmFlashSize {
    /// Address containing the 16-bit flash size value.  The flash size is
    /// the upper 16 bits of this value, so stored at `0x1FFF_7A22`.  We store
    /// a 4-bit aligned address.
    pub const STM32F4_ADDRESS_OF_U16: u32 = 0x1FFF_7A20;
    pub const STM32F1_ADDRESS_OF_U16: u32 = 0x1FFF_F7E0;

    /// Get the initial address for reading the Flash Size based on the STM32
    /// family.
    pub fn addr_from_family(family: StmFamily) -> Option<u32> {
        match family {
            StmFamily::F4 => Some(Self::STM32F4_ADDRESS_OF_U16),
            StmFamily::F1 => Some(Self::STM32F1_ADDRESS_OF_U16),
            StmFamily::Unknown => None,
        }
    }

    /// Create new Flash Size from raw 16-bit value
    pub fn new(raw: u16) -> Self {
        Self { raw }
    }

    /// Get raw Flash Size value
    pub fn raw(&self) -> u16 {
        self.raw
    }

    /// Get Flash size in bytes
    pub fn size_bytes(&self) -> u32 {
        (self.raw as u32) * 1024
    }

    /// Get Flash size in KB
    pub fn size_kb(&self) -> u32 {
        self.raw as u32
    }
}

impl fmt::Display for StmFlashSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flash Size: {} KB", self.size_kb())
    }
}

/// STM32 device details
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StmDetails {
    /// The MCU Device ID
    mcu: StmDeviceId,

    /// The IDCODE (DPIDR value)
    idcode: IdCode,

    /// The Unique ID if available
    uid: Option<StmUniqueId>,

    /// The Flash Size if available
    flash_size: Option<StmFlashSize>,
}

impl StmDetails {
    /// Create new a new `StmDetails` instance
    pub fn new(
        mcu: StmDeviceId,
        idcode: IdCode,
        uid: Option<StmUniqueId>,
        flash_size: Option<StmFlashSize>,
    ) -> Self {
        Self {
            mcu,
            idcode,
            uid,
            flash_size,
        }
    }

    pub fn idcode(&self) -> &IdCode {
        &self.idcode
    }

    pub fn get_cortex(&self) -> Option<Cortex> {
        Cortex::from_idcode(self.idcode)
    }

    /// Get the MCU Device ID
    pub fn mcu(&self) -> &StmDeviceId {
        &self.mcu
    }

    /// Get the Unique ID if available
    pub fn uid(&self) -> Option<StmUniqueId> {
        self.uid
    }

    /// Returns the flash size in bytes if available
    pub fn flash_size_bytes(&self) -> Option<u32> {
        self.flash_size.map(|size| size.size_bytes())
    }

    /// Returns the Flash Size in KB if available
    pub fn flash_size_kb(&self) -> Option<StmFlashSize> {
        self.flash_size
    }

    /// Get the base flash address for this MCU
    pub fn flash_base(&self) -> Option<u32> {
        match self.mcu.family() {
            StmFamily::F4 => Some(STM32F4_FLASH_BASE),
            StmFamily::F1 => Some(STM32F1_FLASH_BASE),
            StmFamily::Unknown => None,
        }
    }

    /// Get the base RAM address for this MCU
    pub fn ram_base(&self) -> Option<u32> {
        match self.mcu.family() {
            StmFamily::F4 => Some(STM32F4_RAM_BASE),
            StmFamily::F1 => Some(STM32F1_RAM_BASE),
            StmFamily::Unknown => None,
        }
    }

    /// Returns whether this is an STM32F4 family MCU
    pub fn is_stm32f4(&self) -> bool {
        matches!(self.mcu.family(), StmFamily::F4)
    }

    /// Returns whether this is an STM32F1 family MCU
    pub fn is_stm32f1(&self) -> bool {
        matches!(self.mcu.family(), StmFamily::F1)
    }

    /// Maximum number of flash sectors for STM32F4 family devices.
    pub const MAX_SECTORS: u8 = 12;

    // STM32F4 sector sizes in bytes.  The same for all supported F4
    // devices.
    const SECTOR_SIZES_BYTES: [u32; Self::MAX_SECTORS as usize] = [
        16 * 1024,  // Sector 0
        16 * 1024,  // Sector 1
        16 * 1024,  // Sector 2
        64 * 1024,  // Sector 3
        128 * 1024, // Sector 4
        128 * 1024, // Sector 5
        128 * 1024, // Sector 6
        128 * 1024, // Sector 7
        128 * 1024, // Sector 8
        128 * 1024, // Sector 9
        128 * 1024, // Sector 10
        128 * 1024, // Sector 11
    ];

    /// Returns the size of the indicated flash sector in bytes if available.
    ///
    /// Only returns the size of the sector if it is within the device's flash
    /// size.
    ///
    /// Arguments:
    /// - `sector`: The sector number to get the size for.
    ///
    /// Returns:
    /// - `Some(size)`: The size of the sector in bytes if it is valid and
    ///   within the device's flash size.
    /// - `None`: If the device is not an STM32F4 family MCU, or if the sector
    ///   is invalid for this device.
    ///
    /// Note that (currently unsupported) F42x and F43x lines have the option
    /// of different sector organisations, not supported by this function.
    pub fn get_sector_size_bytes(&self, sector: u8) -> Option<u32> {
        if !self.is_stm32f4() {
            return None;
        }

        // Get the value
        let value = if sector < Self::SECTOR_SIZES_BYTES.len() as u8 {
            Self::SECTOR_SIZES_BYTES[sector as usize]
        } else {
            return None;
        };

        // If we can't get the device's flash size, return None for the sector
        // size, as we can't check.
        let flash_size = self.flash_size_bytes()? as usize;

        // Sum up all sector sizes up to the given sector, and check it's
        // within the flash size.  This requires iteration
        let mut total_size: usize = 0;
        for size in Self::SECTOR_SIZES_BYTES.iter().take(sector as usize + 1) {
            total_size += *size as usize;
            if total_size > flash_size {
                return None; // Sector exceeds flash size
            }
        }
        Some(value)
    }

    /// Returns the size of the indicated flash sector in KB if available.
    ///
    /// This is a convenience function that uses
    /// [`Self::get_sector_size_bytes()`] to return the flash sector size in
    /// KB.
    ///
    /// Arguments:
    /// - `sector`: The sector number to get the size for.
    ///
    /// Returns:
    /// - `Some(size)`: The size of the sector in KB if it is valid and
    ///   the device's flash size.
    /// - `None`: If the device is not an STM32F4 family MCU, or if the sector
    ///   is invalid for this device.
    pub fn get_sector_size_kb(&self, sector: u8) -> Option<u32> {
        self.get_sector_size_bytes(sector).map(|size| size / 1024)
    }

    /// Returns the flash sector number or numbers for the given word range.
    ///
    /// Note that the word range is relative to the start of the flash, and the
    /// range is _inclusive_ of the entirety of the end word.
    ///
    /// Arguments:
    /// - `range`: A range of words (inclusive) to get the sectors for.
    /// - `sectors`: A mutable array of sectors to fill with the sector
    ///   numbers.
    ///
    /// Returns:
    /// - `Some(sector_count)`: The number of sectors found in the range,
    ///   filled in the `sectors` array.
    /// - `None`: If the device is not an STM32F4 family MCU, or if the range
    ///   is invalid or exceeds the flash size.
    pub fn get_sectors_from_word_range(
        &self,
        range: RangeInclusive<u32>,
        sectors: &mut [u8; Self::MAX_SECTORS as usize],
    ) -> Option<usize> {
        if !self.is_stm32f4() {
            return None;
        }

        // Convert the range to bytes
        let start_word = *range.start();
        let start_bytes = start_word * 4;
        let end_word = *range.end();
        let end_bytes = end_word * 4;

        // Do some checking
        if start_word > end_word {
            return None; // Invalid range
        }
        let flash_size = self.flash_size_bytes()?;
        if end_bytes + 4 > flash_size {
            return None; // Range exceeds flash size
        }

        // Use SECTOR_SIZES_BYTES to determine the sector or sectors
        let mut sector_count = 0;
        let mut current_address = 0u32;
        let range_end_bytes = end_bytes + 3; // Inclusive end of last word

        for (sector_num, &sector_size) in Self::SECTOR_SIZES_BYTES.iter().enumerate() {
            let sector_start = current_address;
            let sector_end = current_address + sector_size - 1;

            // Check if this sector overlaps with our byte range
            if start_bytes <= sector_end && range_end_bytes >= sector_start {
                sectors[sector_count] = sector_num as u8;
                sector_count += 1;
            }

            current_address += sector_size;

            // Early exit if we've gone past our range
            if current_address > range_end_bytes {
                break;
            }
        }

        Some(sector_count)
    }

    /// Returns the expected IDR value for this STM32 family.
    ///
    /// Returns:
    /// - `Some(Idr::IDR_AHB_AP_CORTEX_M4)`: If this is an STM32F4 family MCU.
    /// - `None`: If this is not an STM32F4 family MCU.
    pub fn expected_idr(&self) -> Option<Idr> {
        if self.is_stm32f4() {
            Some(IDR_AHB_AP_CORTEX_M4)
        } else if self.is_stm32f1() {
            Some(IDR_AHB_AP_CORTEX_M3)
        } else {
            None
        }
    }
}

impl fmt::Display for StmDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.mcu)?;

            if let Some(flash_size) = self.flash_size {
                write!(f, " {} KB", flash_size.size_kb())?;
            } else {
                write!(f, " Flash Size: unknown ")?;
            };

            if let Some(uid) = &self.uid {
                write!(f, " {uid}")
            } else {
                write!(f, " UID: unknown ")
            }
        } else {
            write!(f, "{}", self.mcu)
        }
    }
}
