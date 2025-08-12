// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! ARM Debug Port Registers

use crate::arm::register::{DpRegister, ReadableRegister, RegisterDescriptor, WritableRegister};
use crate::{register_data_r, register_data_rw, register_data_w};
use alloc::{format, string::String};
use core::fmt;

/// IDCODE Register descriptor (read-only)
pub struct IdCodeRegister;

impl RegisterDescriptor for IdCodeRegister {
    const ADDRESS: u8 = 0x00;
    type Value = IdCode;
}

impl ReadableRegister for IdCodeRegister {}
impl DpRegister for IdCodeRegister {}

/// ARM Debug Port IDCODE register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdCode(u32);

impl IdCode {
    pub const fn new(value: u32) -> Self {
        IdCode(value)
    }

    pub fn data(&self) -> u32 {
        self.0
    }

    /// Get revision field (bits 31:28)
    pub fn revision(&self) -> u8 {
        ((self.0 >> 28) & 0xF) as u8
    }

    /// Get part number (bits 27:20)
    pub fn part_number(&self) -> u8 {
        ((self.0 >> 20) & 0xFF) as u8
    }

    /// Get version (bits 15:12)
    pub fn version(&self) -> u8 {
        ((self.0 >> 12) & 0xF) as u8
    }

    /// Get MIN (bit 16)
    pub fn min(&self) -> bool {
        (self.0 & (1 << 16)) != 0
    }

    /// Get JEDEC desginer ID (bits 11:1)
    pub fn designer_id(&self) -> u16 {
        ((self.0 >> 1) & 0x7FF) as u16
    }

    /// Check if LSB is set (should always be 1 for valid IDCODE)
    pub fn is_valid(&self) -> bool {
        (self.0 & 1) == 1
    }

    /// Get RAO (bit 0)
    pub fn rao(&self) -> bool {
        (self.0 & 1) != 0
    }

    /// Get manufacturer name if known
    pub fn designer_name(&self) -> &'static str {
        match self.designer_id() {
            0x23B => "ARM Ltd",
            _ => "Unknown",
        }
    }

    /// Get part description if known
    pub fn part_description(&self) -> &'static str {
        if self.designer_id() == 0x23B {
            if self.part_number() == 0xBA {
                match self.version() {
                    0 => "ARM Debug Port v0",
                    1 => "ARM Debug Port v1",
                    2 => "ARM Debug Port v2",
                    _ => "Unknown ARM Debug Port Version",
                }
            } else {
                "unknown"
            }
        } else {
            "unknown"
        }
    }

    /// Check if this is an ARM Debug Port
    pub fn is_arm_debug_port(&self) -> bool {
        self.designer_id() == 0x23B && self.part_number() == 0xBA
    }

    pub const fn from_u32(value: u32) -> Self {
        IdCode(value)
    }
}

impl From<u32> for IdCode {
    fn from(value: u32) -> Self {
        Self::from_u32(value)
    }
}

impl fmt::Display for IdCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            if !self.is_valid() {
                return write!(f, "Invalid IDCODE: 0x{:08X} (LSB not set)", self.0);
            }

            write!(f, "0x{:08X} {}", self.0, self.part_description())
        } else {
            write!(f, "0x{:08X}", self.0)
        }
    }
}

impl fmt::LowerHex for IdCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.0)
    }
}

impl fmt::UpperHex for IdCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}", self.0)
    }
}

/// ABORT Register descriptor (write-only)
pub struct AbortRegister;

impl RegisterDescriptor for AbortRegister {
    const ADDRESS: u8 = 0x00;
    type Value = Abort;
}

impl WritableRegister for AbortRegister {}
impl DpRegister for AbortRegister {}

/// ARM Debug Port ABORT register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Abort(u32);

// Standard register data impls
register_data_w!(Abort);

impl Abort {
    const STKCMPCLR: u32 = 1 << 1;
    const STKERRCLR: u32 = 1 << 2;
    const WDERRCLR: u32 = 1 << 3;
    const ORUNERRCLR: u32 = 1 << 4;

    /// Set sticky compare clear flag
    pub fn set_stkcmpclr(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::STKCMPCLR;
        } else {
            self.0 &= !Self::STKCMPCLR;
        }
    }

    /// Set sticky error clear flag
    pub fn set_stkerrclr(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::STKERRCLR;
        } else {
            self.0 &= !Self::STKERRCLR;
        }
    }

    /// Set write data error clear flag
    pub fn set_wderrclr(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::WDERRCLR;
        } else {
            self.0 &= !Self::WDERRCLR;
        }
    }

    /// Set overrun error clear flag
    pub fn set_orunerrclr(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::ORUNERRCLR;
        } else {
            self.0 &= !Self::ORUNERRCLR;
        }
    }
}

/// CTRL/STAT Register descriptor (read-write)
pub struct CtrlStatRegister;

impl RegisterDescriptor for CtrlStatRegister {
    const ADDRESS: u8 = 0x04;
    type Value = CtrlStat;
}

impl ReadableRegister for CtrlStatRegister {}
impl WritableRegister for CtrlStatRegister {}
impl DpRegister for CtrlStatRegister {}

/// ARM Debug Port CTRL/STAT register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CtrlStat(u32);

// Standard register data impls
register_data_rw!(CtrlStat);

impl CtrlStat {
    // Field masks and shifts
    const ORUNDETECT: u32 = 1 << 0;
    const STICKYORUN: u32 = 1 << 1;

    const TRNMODE_MASK: u32 = 0b11;
    const TRNMODE_SHIFT: u32 = 2;

    const STICKYCMP: u32 = 1 << 4;
    const STICKYERR: u32 = 1 << 5;
    const READOK: u32 = 1 << 6;
    const WDATAERR: u32 = 1 << 7;

    const MASKLANE_MASK: u32 = 0b1111;
    const MASKLANE_SHIFT: u32 = 8;

    const TRNCNT_MASK: u32 = 0b111111111111;
    const TRNCNT_SHIFT: u32 = 12;

    const CDBGRSTREQ: u32 = 1 << 26;
    const CDBGRSTACK: u32 = 1 << 27;
    const CDBGPWRUPREQ: u32 = 1 << 28;
    const CDBGPWRUPACK: u32 = 1 << 29;
    const CSYSPWRUPREQ: u32 = 1 << 30;
    const CSYSPWRUPACK: u32 = 1 << 31;

    // Transaction mode values
    pub const TRNMODE_NORMAL: u32 = 0b00;
    pub const TRNMODE_VERIFY: u32 = 0b01;
    pub const TRNMODE_COMPARE: u32 = 0b10;

    /// Get raw register value
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Get overrun detection enable
    pub fn orundetect(&self) -> bool {
        self.0 & Self::ORUNDETECT != 0
    }

    /// Get sticky overrun flag
    pub fn stickyorun(&self) -> bool {
        self.0 & Self::STICKYORUN != 0
    }

    /// Get transaction mode
    pub fn trnmode(&self) -> u32 {
        (self.0 >> Self::TRNMODE_SHIFT) & Self::TRNMODE_MASK
    }

    /// Get sticky compare flag
    pub fn stickycmp(&self) -> bool {
        self.0 & Self::STICKYCMP != 0
    }

    /// Get sticky error flag
    pub fn stickyerr(&self) -> bool {
        self.0 & Self::STICKYERR != 0
    }

    /// Get read OK flag
    pub fn readok(&self) -> bool {
        self.0 & Self::READOK != 0
    }

    /// Get write data error flag
    pub fn wdataerr(&self) -> bool {
        self.0 & Self::WDATAERR != 0
    }

    /// Get mask lane value
    pub fn masklane(&self) -> u32 {
        (self.0 >> Self::MASKLANE_SHIFT) & Self::MASKLANE_MASK
    }

    /// Get transaction count
    pub fn trncnt(&self) -> u32 {
        (self.0 >> Self::TRNCNT_SHIFT) & Self::TRNCNT_MASK
    }

    /// Get debug reset request
    pub fn cdbgrstreq(&self) -> bool {
        self.0 & Self::CDBGRSTREQ != 0
    }

    /// Get debug reset acknowledge
    pub fn cdbgrstack(&self) -> bool {
        self.0 & Self::CDBGRSTACK != 0
    }

    /// Get debug power-up request
    pub fn cdbgpwrupreq(&self) -> bool {
        self.0 & Self::CDBGPWRUPREQ != 0
    }

    /// Get debug power-up acknowledge
    pub fn cdbgpwrupack(&self) -> bool {
        self.0 & Self::CDBGPWRUPACK != 0
    }

    /// Get system power-up request
    pub fn csyspwrupreq(&self) -> bool {
        self.0 & Self::CSYSPWRUPREQ != 0
    }

    /// Get system power-up acknowledge
    pub fn csyspwrupack(&self) -> bool {
        self.0 & Self::CSYSPWRUPACK != 0
    }

    pub fn has_errors(&self) -> bool {
        self.stickyorun()
            || self.stickycmp()
            || self.stickyerr()
            || self.wdataerr()
            || self.stickyorun()
    }

    // Setters
    /// Set overrun detection enable
    pub fn set_orundetect(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::ORUNDETECT;
        } else {
            self.0 &= !Self::ORUNDETECT;
        }
    }

    /// Set sticky overrun flag
    pub fn set_stickyorun(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::STICKYORUN;
        } else {
            self.0 &= !Self::STICKYORUN;
        }
    }

    /// Set transaction mode
    pub fn set_trnmode(&mut self, mode: u32) {
        self.0 = (self.0 & !(Self::TRNMODE_MASK << Self::TRNMODE_SHIFT))
            | ((mode & Self::TRNMODE_MASK) << Self::TRNMODE_SHIFT);
    }

    /// Set sticky compare flag
    pub fn set_stickycmp(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::STICKYCMP;
        } else {
            self.0 &= !Self::STICKYCMP;
        }
    }

    /// Set sticky error flag
    pub fn set_stickyerr(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::STICKYERR;
        } else {
            self.0 &= !Self::STICKYERR;
        }
    }

    /// Set mask lane value
    pub fn set_masklane(&mut self, mask: u32) {
        self.0 = (self.0 & !(Self::MASKLANE_MASK << Self::MASKLANE_SHIFT))
            | ((mask & Self::MASKLANE_MASK) << Self::MASKLANE_SHIFT);
    }

    /// Set transaction count
    pub fn set_trncnt(&mut self, count: u32) {
        self.0 = (self.0 & !(Self::TRNCNT_MASK << Self::TRNCNT_SHIFT))
            | ((count & Self::TRNCNT_MASK) << Self::TRNCNT_SHIFT);
    }

    /// Set debug reset request
    pub fn set_cdbgrstreq(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::CDBGRSTREQ;
        } else {
            self.0 &= !Self::CDBGRSTREQ;
        }
    }

    /// Set debug power-up request
    pub fn set_cdbgpwrupreq(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::CDBGPWRUPREQ;
        } else {
            self.0 &= !Self::CDBGPWRUPREQ;
        }
    }

    /// Set system power-up request
    pub fn set_csyspwrupreq(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::CSYSPWRUPREQ;
        } else {
            self.0 &= !Self::CSYSPWRUPREQ;
        }
    }

    // Formatters
    /// Get error state description
    pub fn error_states(&self) -> String {
        let mut errors = [""; 5];
        let mut count = 0;

        if self.stickyorun() {
            errors[count] = "STICKYORUN";
            count += 1;
        }
        if self.stickycmp() {
            errors[count] = "STICKYCMP";
            count += 1;
        }
        if self.stickyerr() {
            errors[count] = "STICKYERR";
            count += 1;
        }
        if self.wdataerr() {
            errors[count] = "WDATAERR";
            count += 1;
        }
        if self.orundetect() {
            errors[count] = "ORUNDETECT";
            count += 1;
        }

        if count == 0 {
            format!("No errors{}", if self.readok() { " (READOK)" } else { "" })
        } else {
            format!("Errors: {}", errors[..count].join(", "))
        }
    }

    /// Get power state description
    pub fn power_states(&self) -> String {
        format!(
            "Debug: {}/{}, System: {}/{}",
            if self.cdbgpwrupreq() { "REQ" } else { "off" },
            if self.cdbgpwrupack() { "ACK" } else { "nak" },
            if self.csyspwrupreq() { "REQ" } else { "off" },
            if self.csyspwrupack() { "ACK" } else { "nak" }
        )
    }

    /// Get transaction state description
    pub fn transaction_state(&self) -> String {
        let mode = match self.trnmode() {
            Self::TRNMODE_NORMAL => "Normal",
            Self::TRNMODE_VERIFY => "Verify",
            Self::TRNMODE_COMPARE => "Compare",
            _ => "Reserved",
        };
        format!(
            "Mode: {}, Count: {}, Mask: 0x{:x}",
            mode,
            self.trncnt(),
            self.masklane()
        )
    }
}

/// SELECT Register descriptor (read-write)
pub struct SelectRegister;

impl RegisterDescriptor for SelectRegister {
    const ADDRESS: u8 = 0x08;
    type Value = Select;
}

impl ReadableRegister for SelectRegister {}
impl WritableRegister for SelectRegister {}
impl DpRegister for SelectRegister {}

/// ARM Debug Port SELECT register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Select(u32);

// Standard register data impls
register_data_rw!(Select);

impl Select {
    // Field masks and shifts
    const APSEL_MASK: u32 = 0xFF;
    const APSEL_SHIFT: u32 = 24;

    pub const DPBANKSEL_MASK: u32 = 0xF;
    pub const DPBANKSEL_SHIFT: u32 = 0;

    pub const APBANKSEL_MASK: u32 = 0xF;
    pub const APBANKSEL_SHIFT: u32 = 4;

    /// Get raw register value
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Get access port select
    pub fn apsel(&self) -> u32 {
        (self.0 >> Self::APSEL_SHIFT) & Self::APSEL_MASK
    }

    /// Get DP bank select
    pub fn dpbanksel(&self) -> u32 {
        (self.0 >> Self::DPBANKSEL_SHIFT) & Self::DPBANKSEL_MASK
    }

    /// Get AP bank select
    pub fn apbanksel(&self) -> u32 {
        (self.0 >> Self::APBANKSEL_SHIFT) & Self::APBANKSEL_MASK
    }

    /// Set access port select
    pub fn set_apsel(&mut self, apsel: u32) {
        self.0 = (self.0 & !(Self::APSEL_MASK << Self::APSEL_SHIFT))
            | ((apsel & Self::APSEL_MASK) << Self::APSEL_SHIFT);
    }

    /// Set DP bank select
    pub fn set_dpbanksel(&mut self, banksel: u8) {
        let banksel = banksel as u32;
        self.0 = (self.0 & !(Self::DPBANKSEL_MASK << Self::DPBANKSEL_SHIFT))
            | ((banksel & Self::DPBANKSEL_MASK) << Self::DPBANKSEL_SHIFT);
    }

    /// Set AP bank select
    pub fn set_apbanksel(&mut self, banksel: u8) {
        let banksel = banksel as u32;
        self.0 = (self.0 & !(Self::APBANKSEL_MASK << Self::APBANKSEL_SHIFT))
            | ((banksel & Self::APBANKSEL_MASK) << Self::APBANKSEL_SHIFT);
    }

    /// Set DP bank select from address
    pub fn set_dpbanksel_from_addr(&mut self, addr: u8) {
        let banksel = (addr >> 4) & 0xF;
        self.set_dpbanksel(banksel);
    }

    /// Set AP bank select from address
    pub fn set_apbanksel_from_addr(&mut self, addr: u8) {
        let banksel = (addr >> 4) & 0xF;
        self.set_apbanksel(banksel);
    }

    /// Get selection information string
    pub fn selection_info(&self) -> String {
        format!(
            "AP: {}, DP Bank: {}, AP Bank: {}",
            self.apsel(),
            self.dpbanksel(),
            self.apbanksel()
        )
    }
}

/// RDBUFF Register descriptor (read-only)
pub struct RdBuffRegister;

impl RegisterDescriptor for RdBuffRegister {
    const ADDRESS: u8 = 0x0C;
    type Value = RdBuff;
}

impl ReadableRegister for RdBuffRegister {}
impl DpRegister for RdBuffRegister {}

/// ARM Debug Port RDBUFF register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RdBuff(u32);

// Standard register data impls
register_data_r!(RdBuff);

impl RdBuff {
    /// Get the buffered data
    pub fn data(&self) -> u32 {
        self.0
    }
}

/// TARGETSEL Register descriptor (write-only, DPv2)
pub struct TargetSelRegister;

impl RegisterDescriptor for TargetSelRegister {
    const ADDRESS: u8 = 0x0C;
    type Value = TargetSel;
}

impl WritableRegister for TargetSelRegister {}
impl DpRegister for TargetSelRegister {}

/// ARM Debug Port RDBUFF register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TargetSel(u32);

register_data_w!(TargetSel);

impl TargetSel {
    pub const fn new(value: u32) -> Self {
        TargetSel(value)
    }

    /// Get the buffered data
    pub fn data(&self) -> u32 {
        self.0
    }
}

impl From<IdCode> for TargetSel {
    fn from(id: IdCode) -> Self {
        TargetSel(id.data())
    }
}

// RP2040 Multi-Drop Targets
pub const TARGET_SEL_RP2040_BASE: u32 = 0x01002927;
pub const TARGET_SEL_RP2040_CORE0: TargetSel = TargetSel(TARGET_SEL_RP2040_BASE);
pub const TARGET_SEL_RP2040_CORE1: TargetSel = TargetSel(0x1 << 28 | TARGET_SEL_RP2040_BASE);
pub const TARGET_SEL_RP2040_RESCUE_DP: TargetSel = TargetSel(0xF << 28 | TARGET_SEL_RP2040_BASE);

pub const TARGET_SEL_RP2040_ALL: [TargetSel; 3] = [
    TARGET_SEL_RP2040_CORE0,
    TARGET_SEL_RP2040_CORE1,
    TARGET_SEL_RP2040_RESCUE_DP,
];
