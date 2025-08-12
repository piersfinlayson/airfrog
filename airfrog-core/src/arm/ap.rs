// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! ARM Generic Access Port Registers

use crate::arm::register::{ApRegister, ReadableRegister, RegisterDescriptor};
use crate::register_data_r;
use alloc::format;
use core::fmt;

/// Access Port Identification Register descriptor
pub struct IdrRegister;

impl RegisterDescriptor for IdrRegister {
    const ADDRESS: u8 = 0xFC;
    type Value = Idr;
}

impl ReadableRegister for IdrRegister {}

impl ApRegister for IdrRegister {}

// Standard register data impls
register_data_r!(Idr);

/// Access Port Identification Register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Idr(u32);

impl Idr {
    // Field masks and shifts
    const REVISION_MASK: u32 = 0xF;
    const REVISION_SHIFT: u32 = 28;

    const DESIGNER_MASK: u32 = 0x7FF;
    const DESIGNER_SHIFT: u32 = 17;

    const CONTINUATION_MASK: u32 = 0xF;
    const CONTINUATION_SHIFT: u32 = 24;

    const IDENTIFICATION_MASK: u32 = 0x7F;
    const IDENTIFICATION_SHIFT: u32 = 17;

    const CLASS_MASK: u32 = 0xF;
    const CLASS_SHIFT: u32 = 13;

    const VARIANT_MASK: u32 = 0xF;
    const VARIANT_SHIFT: u32 = 4;

    const TYPE_MASK: u32 = 0xF;
    const TYPE_SHIFT: u32 = 0;

    /// No Access Port present
    pub const CLASS_NONE: u32 = 0x0;
    /// Memory Access Port
    pub const CLASS_MEM_AP: u32 = 0x8;

    pub const AP_TYPE_COM_AP: u32 = 0x0;
    pub const AP_TYPE_AMBA_AHB3: u32 = 0x1;
    pub const AP_TYPE_AMBA_APB2_3: u32 = 0x2;
    pub const AP_TYPE_AMBA_AXI3_4: u32 = 0x4;
    pub const AP_TYPE_AMBA_AHB5: u32 = 0x5;
    pub const AP_TYPE_AMBA_APB4_5: u32 = 0x6;
    pub const AP_TYPE_AMBA_AXI5: u32 = 0x7;
    pub const AP_TYPE_AMBA_AHB5_EN_HPROT: u32 = 0x8;

    /// Create a new IDR from a raw value
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Get revision field (bits 31:28)
    pub fn revision(&self) -> u32 {
        (self.0 >> Self::REVISION_SHIFT) & Self::REVISION_MASK
    }

    /// Get designer field (bits 27:17)
    pub fn designer(&self) -> u32 {
        (self.0 >> Self::DESIGNER_SHIFT) & Self::DESIGNER_MASK
    }

    /// Get continuation code (bits 27:24)
    pub fn continuation(&self) -> u32 {
        (self.0 >> Self::CONTINUATION_SHIFT) & Self::CONTINUATION_MASK
    }

    /// Get identification field (bits 23:17)
    pub fn identification(&self) -> u32 {
        (self.0 >> Self::IDENTIFICATION_SHIFT) & Self::IDENTIFICATION_MASK
    }

    /// Get class field (bits 16:13)
    pub fn class(&self) -> u32 {
        (self.0 >> Self::CLASS_SHIFT) & Self::CLASS_MASK
    }

    /// Get variant field (bits 7:4)
    pub fn variant(&self) -> u32 {
        (self.0 >> Self::VARIANT_SHIFT) & Self::VARIANT_MASK
    }

    /// Get AP type field (bits 3:0)
    pub fn ap_type(&self) -> u32 {
        (self.0 >> Self::TYPE_SHIFT) & Self::TYPE_MASK
    }

    /// Get formatted information string
    pub fn idr_info(&self) -> alloc::string::String {
        format!(
            "Designer: 0x{:03X}, Continuation: 0x{:01X} Identification: 0x{:02X} Class: 0x{:X}, Type: 0x{:X}, Variant: 0x{:X}, Rev: 0x{:X}",
            self.designer(),
            self.continuation(),
            self.identification(),
            self.class(),
            self.ap_type(),
            self.variant(),
            self.revision()
        )
    }
}

/// ARM Cortex-M0 AHB-AP IDR value
pub const IDR_AHB_AP_CORTEX_M0: Idr = Idr::new(0x04770031);

/// ARM Cortex-M3 AHB-AP IDR value
pub const IDR_AHB_AP_CORTEX_M3: Idr = Idr::new(0x24770011);

/// ARM Cortex-M4 AHB-AP IDR value
pub const IDR_AHB_AP_CORTEX_M4: Idr = Idr::new(0x24770011);

/// ARM Cortex-M33 AHB-AP IDR value
pub const IDR_AHB_AP_CORTEX_M33: Idr = Idr::new(0x24770011);

pub const IDR_AHB_AP_KNOWN: [Idr; 4] = [
    IDR_AHB_AP_CORTEX_M0,
    IDR_AHB_AP_CORTEX_M3,
    IDR_AHB_AP_CORTEX_M4,
    IDR_AHB_AP_CORTEX_M33,
];
