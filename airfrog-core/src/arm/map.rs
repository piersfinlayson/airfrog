// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! ARM Memory Access Port Registers

use crate::arm::register::{ApRegister, ReadableRegister, RegisterDescriptor, WritableRegister};
use crate::register_data_rw;
use alloc::{format, string::String};
use core::fmt;

/// Control/Status Word Register descriptor (read-write)
pub struct CswRegister;

impl RegisterDescriptor for CswRegister {
    const ADDRESS: u8 = 0x00;
    type Value = Csw;
}

impl ReadableRegister for CswRegister {}
impl WritableRegister for CswRegister {}
impl ApRegister for CswRegister {}

/// Control/Status Word register data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Csw(u32);

// Standard register data impls
register_data_rw!(Csw);

impl Csw {
    // Field masks and shifts
    const SIZE_MASK: u32 = 0b111;
    const SIZE_SHIFT: u32 = 0;

    const ADDRINC_MASK: u32 = 0b11;
    const ADDRINC_SHIFT: u32 = 4;

    const DEVICE_EN: u32 = 1 << 6;
    const TR_IN_PROG: u32 = 1 << 7;

    const MODE_MASK: u32 = 0b1111;
    const MODE_SHIFT: u32 = 8;

    const TYPE_MASK: u32 = 0b111;
    const TYPE_SHIFT: u32 = 12;

    const MTE: u32 = 1 << 15;

    // SPIDEN is read only
    const SPIDEN: u32 = 1 << 23;

    const PROT_MASK: u32 = 0b1111111;
    const PROT_SHIFT: u32 = 24;

    const DBG_SW_ENABLE: u32 = 1 << 31;

    const RESERVED_HIGH: u32 = 1 << 24;

    // Size values
    pub const SIZE_8BIT: u32 = 0b000;
    pub const SIZE_16BIT: u32 = 0b001;
    pub const SIZE_32BIT: u32 = 0b010;
    pub const SIZE_64BIT: u32 = 0b011;
    pub const SIZE_128BIT: u32 = 0b100;
    pub const SIZE_256BIT: u32 = 0b101;

    // Address increment values
    pub const ADDRINC_OFF: u32 = 0b00;
    pub const ADDRINC_SINGLE: u32 = 0b01;
    pub const ADDRINC_PACKED: u32 = 0b10;

    // Prot values
    pub const PROT_MASTER_DEBUG: u32 = 1 << 5;
    pub const PROT_BIT_1: u32 = 1 << 1;

    /// Get raw register value
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Get size field
    pub fn size(&self) -> u32 {
        (self.0 >> Self::SIZE_SHIFT) & Self::SIZE_MASK
    }

    /// Get address increment field
    pub fn addrinc(&self) -> u32 {
        (self.0 >> Self::ADDRINC_SHIFT) & Self::ADDRINC_MASK
    }

    /// Get device enable flag
    pub fn device_en(&self) -> bool {
        self.0 & Self::DEVICE_EN != 0
    }

    /// Get transfer in progress flag
    pub fn tr_in_prog(&self) -> bool {
        self.0 & Self::TR_IN_PROG != 0
    }

    /// Get mode field
    pub fn mode(&self) -> u32 {
        (self.0 >> Self::MODE_SHIFT) & Self::MODE_MASK
    }

    /// Get type field
    pub fn type_bits(&self) -> u32 {
        (self.0 >> Self::TYPE_SHIFT) & Self::TYPE_MASK
    }

    /// Get MTE flag
    pub fn mte(&self) -> bool {
        self.0 & Self::MTE != 0
    }

    /// Get SPIDEN flag
    pub fn spiden(&self) -> bool {
        self.0 & Self::SPIDEN != 0
    }

    /// Get protection field
    pub fn prot(&self) -> u32 {
        (self.0 >> Self::PROT_SHIFT) & Self::PROT_MASK
    }

    /// Get debug software enable flag
    pub fn dbg_sw_enable(&self) -> bool {
        self.0 & Self::DBG_SW_ENABLE != 0
    }

    // Setters
    pub fn set_reserved_high(&mut self) {
        self.0 |= Self::RESERVED_HIGH;
    }

    /// Set size field
    pub fn set_size(&mut self, size: u32) {
        self.0 = (self.0 & !(Self::SIZE_MASK << Self::SIZE_SHIFT))
            | ((size & Self::SIZE_MASK) << Self::SIZE_SHIFT);
    }

    /// Set address increment field
    pub fn set_addrinc(&mut self, addrinc: u32) {
        self.0 = (self.0 & !(Self::ADDRINC_MASK << Self::ADDRINC_SHIFT))
            | ((addrinc & Self::ADDRINC_MASK) << Self::ADDRINC_SHIFT);
    }

    /// Set device enable flag
    pub fn set_device_en(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::DEVICE_EN;
        } else {
            self.0 &= !Self::DEVICE_EN;
        }
    }

    /// Set mode field
    pub fn set_mode(&mut self, mode: u32) {
        self.0 = (self.0 & !(Self::MODE_MASK << Self::MODE_SHIFT))
            | ((mode & Self::MODE_MASK) << Self::MODE_SHIFT);
    }

    /// Set type field
    pub fn set_type(&mut self, type_bits: u32) {
        self.0 = (self.0 & !(Self::TYPE_MASK << Self::TYPE_SHIFT))
            | ((type_bits & Self::TYPE_MASK) << Self::TYPE_SHIFT);
    }

    /// Set MTE flag
    pub fn set_mte(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::MTE;
        } else {
            self.0 &= !Self::MTE;
        }
    }

    /// Set SPIDEN flag
    pub fn set_spiden(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::SPIDEN;
        } else {
            self.0 &= !Self::SPIDEN;
        }
    }

    /// Set protection field
    pub fn set_prot(&mut self, prot: u32) {
        self.0 = (self.0 & !(Self::PROT_MASK << Self::PROT_SHIFT))
            | ((prot & Self::PROT_MASK) << Self::PROT_SHIFT);
    }

    /// Set debug software enable flag
    pub fn set_dbg_sw_enable(&mut self, enable: bool) {
        if enable {
            self.0 |= Self::DBG_SW_ENABLE;
        } else {
            self.0 &= !Self::DBG_SW_ENABLE;
        }
    }

    /// Get transfer configuration description
    pub fn transfer_config(&self) -> String {
        let size = match self.size() {
            Self::SIZE_8BIT => "8-bit",
            Self::SIZE_16BIT => "16-bit",
            Self::SIZE_32BIT => "32-bit",
            Self::SIZE_64BIT => "64-bit",
            Self::SIZE_128BIT => "128-bit",
            Self::SIZE_256BIT => "256-bit",
            _ => "Reserved",
        };

        let addrinc = match self.addrinc() {
            Self::ADDRINC_OFF => "Off",
            Self::ADDRINC_SINGLE => "Single",
            Self::ADDRINC_PACKED => "Packed",
            _ => "Reserved",
        };

        format!("Size: {size}, AddrInc: {addrinc}")
    }

    /// Get status flags description
    pub fn status_flags(&self) -> String {
        let mut flags = [""; 6];
        let mut count = 0;

        if self.device_en() {
            flags[count] = "DEVICE_EN";
            count += 1;
        }
        if self.tr_in_prog() {
            flags[count] = "TR_IN_PROG";
            count += 1;
        }
        if self.mte() {
            flags[count] = "MTE";
            count += 1;
        }
        if self.spiden() {
            flags[count] = "SPIDEN";
            count += 1;
        }
        if self.dbg_sw_enable() {
            flags[count] = "DBG_SW_ENABLE";
            count += 1;
        }

        if count == 0 {
            "No flags set".into()
        } else {
            format!("Flags: {}", flags[..count].join(", "))
        }
    }

    /// Get security state description
    pub fn security_state(&self) -> String {
        format!(
            "PROT: 0x{:02x}, SPIDEN: {}, DBG_SW: {}",
            self.prot(),
            if self.spiden() { "Y" } else { "N" },
            if self.dbg_sw_enable() { "Y" } else { "N" }
        )
    }
}

impl Default for Csw {
    fn default() -> Self {
        let mut csw = Csw(0);
        csw.set_reserved_high();
        csw.set_prot(Self::PROT_MASTER_DEBUG | Self::PROT_BIT_1);
        csw.set_size(Self::SIZE_32BIT);
        csw.set_addrinc(Self::ADDRINC_OFF);
        csw.set_device_en(true);

        csw
    }
}

/// Transfer Address Register descriptor (read-write)
pub struct TarRegister;

impl RegisterDescriptor for TarRegister {
    const ADDRESS: u8 = 0x04;
    type Value = Tar;
}

impl ReadableRegister for TarRegister {}
impl WritableRegister for TarRegister {}
impl ApRegister for TarRegister {}

/// Transfer Address Register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tar(u32);

// Standard register data impls
register_data_rw!(Tar);

impl Tar {
    /// Get raw register value
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Get target address
    pub fn target_address(&self) -> u32 {
        self.0
    }

    /// Set target address
    pub fn set_target_address(&mut self, address: u32) {
        self.0 = address;
    }
}

/// Data Read/Write Register descriptor (read-write)
pub struct DrwRegister;

impl RegisterDescriptor for DrwRegister {
    const ADDRESS: u8 = 0x0C;
    type Value = Drw;
}

impl ReadableRegister for DrwRegister {}
impl WritableRegister for DrwRegister {}
impl ApRegister for DrwRegister {}

/// Data Read/Write Register data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Drw(u32);

// Standard register data impls
register_data_rw!(Drw);

impl Drw {
    /// Get raw register value
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Get data value
    pub fn data(&self) -> u32 {
        self.0
    }

    /// Set data value
    pub fn set_data(&mut self, data: u32) {
        self.0 = data;
    }
}

/// Banked Data Register 0 descriptor (read-write)
pub struct Bd0Register;

impl RegisterDescriptor for Bd0Register {
    const ADDRESS: u8 = 0x10;
    type Value = BankedData;
}

impl ReadableRegister for Bd0Register {}
impl WritableRegister for Bd0Register {}
impl ApRegister for Bd0Register {}

/// Banked Data Register 1 descriptor (read-write)
pub struct Bd1Register;

impl RegisterDescriptor for Bd1Register {
    const ADDRESS: u8 = 0x14;
    type Value = BankedData;
}

impl ReadableRegister for Bd1Register {}
impl WritableRegister for Bd1Register {}
impl ApRegister for Bd1Register {}

/// Banked Data Register 2 descriptor (read-write)
pub struct Bd2Register;

impl RegisterDescriptor for Bd2Register {
    const ADDRESS: u8 = 0x18;
    type Value = BankedData;
}

impl ReadableRegister for Bd2Register {}
impl WritableRegister for Bd2Register {}
impl ApRegister for Bd2Register {}

/// Banked Data Register 3 descriptor (read-write)
pub struct Bd3Register;

impl RegisterDescriptor for Bd3Register {
    const ADDRESS: u8 = 0x1C;
    type Value = BankedData;
}

impl ReadableRegister for Bd3Register {}
impl WritableRegister for Bd3Register {}
impl ApRegister for Bd3Register {}

/// MEM-AP Banked Data register data
///
/// Used as based for [`Bd0Register`], [`Bd1Register`], [`Bd2Register`], and
/// [`Bd3Register`].

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BankedData(u32);

// Standard register data impls
register_data_rw!(BankedData);

impl BankedData {
    /// Get data value
    pub fn data(&self) -> u32 {
        self.0
    }

    /// Set data value
    pub fn set_data(&mut self, data: u32) {
        self.0 = data;
    }
}
