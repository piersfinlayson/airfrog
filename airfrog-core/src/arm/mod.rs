// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! Contains ARM specific objects and routines

pub mod ap;
pub mod dp;
pub mod map;
pub mod register;

use core::fmt;

use dp::IdCode;

/// ARM Cortex core type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cortex {
    /// Cortex-M0
    M0,
    /// Cortex-M3
    M3,
    /// Cortex-M4
    M4,
    /// Cortex-M33
    M33,
}

impl Cortex {
    pub const IDCODE_M0: IdCode = IdCode::from_u32(0x0BC12477);
    pub const IDCODE_M3: IdCode = IdCode::from_u32(0x1BA01477);
    pub const IDCODE_M4: IdCode = IdCode::from_u32(0x2BA01477);
    pub const IDCODE_M33: IdCode = IdCode::from_u32(0x4C013477);

    /// Returns the DPIDR IDCODE for this core type
    pub fn idcode(&self) -> IdCode {
        match self {
            Cortex::M0 => Self::IDCODE_M0,
            Cortex::M3 => Self::IDCODE_M3,
            Cortex::M4 => Self::IDCODE_M4,
            Cortex::M33 => Self::IDCODE_M33,
        }
    }

    /// Returns the core type as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Cortex::M0 => "Cortex-M0",
            Cortex::M3 => "Cortex-M3",
            Cortex::M4 => "Cortex-M4",
            Cortex::M33 => "Cortex-M33",
        }
    }

    pub fn from_idcode(idcode: IdCode) -> Option<Cortex> {
        match idcode {
            Self::IDCODE_M0 => Some(Cortex::M0),
            Self::IDCODE_M3 => Some(Cortex::M3),
            Self::IDCODE_M4 => Some(Cortex::M4),
            Self::IDCODE_M33 => Some(Cortex::M33),
            _ => None,
        }
    }
}

impl fmt::Display for Cortex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ARM {}", self.as_str())
    }
}
