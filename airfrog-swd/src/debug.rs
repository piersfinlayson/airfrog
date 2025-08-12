// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! SWD (Wire) Debug Interface
//!
//! This module implements a high-level SWD (Serial Wire Debug) interface for
//! communicating with ARM devices.  Its aim is to provide a simple and
//! user-friendly API for debugging, programming and co-processing with
//! ARM-based MCUs.
//!
//! If this module does not give you the control you need, you can use the
//! [`SwdInterface`] object directly for lower-level SWD access.
//!
//! To combine the used of this module with [`SwdInterface`], you should create
//! the [`DebugInterface`] object using the `new()` method, and then use
//! `swd_if()` to access the underlying [`SwdInterface`] object as required.

use alloc::format;
use core::result::Result;
use embassy_time::{Duration, Timer};
use esp_hal::gpio::{InputPin, OutputPin};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use airfrog_core::Mcu;
use airfrog_core::arm::ap::{Idr, IdrRegister};
use airfrog_core::arm::dp::{IdCode, TARGET_SEL_RP2040_ALL};
use airfrog_core::stm::StmFamily;
use airfrog_core::stm::{Stm32F4FlashCr, Stm32F4FlashKeyr, Stm32F4FlashSr};

use crate::SwdError;
use crate::interface::SwdInterface;

#[doc(inline)]
pub use crate::protocol::{SwdProtocol, Version};

// Enum used internally to ensure type safety when implementing byte multiple
// operations
#[derive(Debug, Clone, PartialEq, Eq)]
enum OperationBytes {
    BYTE1,
    BYTE2,
    BYTE4,
}

/// ARM Debug Interface object
///
/// This is used by applications to perform operations over SWD.  It can be
/// used to debug, program, and co-process along-side ARM-based MCUs.
///
/// It provides a high-level interface for SWD operations, such as powering up
/// the debug domain, configuring the MEM-AP, and reading and writing memory,
/// which, in turn, can be used to reprogram, control, hijack and otherwise
/// interact with the target device.
///
/// The simplest way to create a `DebugInterface` is to use the
/// [`Self::from_pins()`] method, which takes the SWDIO and SWCLK pins as
/// arguments:
///
/// ```rust
/// use airfrog_swd::debug::DebugInterface;
///
/// let peripherals = esp_hal::init(config);
/// let swdio_pin = peripherals.GPIO0;
/// let swclk_pin = peripherals.GPIO1;
/// let mut debug = DebugInterface::from_pins(swdio_pin, swclk_pin);
///
/// debug.initialize_swd_target().await?;
///
/// let value = debug.read_mem(0x2000_0000).await?;
/// esp_println!::println!("Value at 0x2000_0000: 0x{value:08X}");
/// ```
#[derive(Debug)]
pub struct DebugInterface<'a> {
    swd: SwdInterface<'a>,
}

impl<'a> DebugInterface<'a> {
    /// Creates a new `DebugInterface` with the given [`SwdInterface`].
    ///
    /// This is useful, for example, if you have already created a
    /// [`SwdInterface`] instance and want to use it with the `DebugInterface`.
    /// However, most applications will want to use the [`Self::from_pins()`]
    /// method instead.
    ///
    /// Returns:
    /// - `DebugInterface`: A new instance of the `DebugInterface` with the
    ///   given `SwdInterface`.
    pub fn new(swd: SwdInterface<'a>) -> Self {
        Self { swd }
    }

    /// Creates a new `DebugInterface` from the given SWDIO and SWCLK pins.
    ///
    /// When creating a `DebugInterface` using this method, you can access the
    /// underlying [`SwdInterface`] (which gives lower-level SWD control] using
    /// the [`Self::swd_if()`] method.
    ///
    /// Returns:
    /// - `DebugInterface`: A new instance of the `DebugInterface` with the
    ///   SWDIO and SWCLK pins configured for SWD communication.
    pub fn from_pins(
        swdio_pin: impl InputPin + OutputPin + 'a,
        swclk_pin: impl OutputPin + 'a,
    ) -> Self {
        let swd = SwdInterface::from_pins(swdio_pin, swclk_pin);
        Self { swd }
    }

    /// Returns a mutable reference to the underlying [`SwdInterface`].
    ///
    /// This allows you to access lower-level SWD operations directly, if
    /// required.
    pub fn swd_if(&mut self) -> &mut SwdInterface<'a> {
        &mut self.swd
    }

    /// Initializes the SWD target device for debugging.
    ///
    /// Supports both v1 and v2 SWD targets, but not multi-drop targets.
    ///
    /// Arguments:
    /// - `version`: The SWD version to use for the target device.
    ///
    /// Returns:
    /// - `Ok(IdCode)`: if successful, sharing the the IDCODE of the target's
    ///   SWD implementation.
    /// - `Err(SwdError)`: if there was an error during the initialization
    ///   sequence.
    pub async fn initialize_swd_target(&mut self, version: Version) -> Result<(), SwdError> {
        // Reset the target device
        self.swd.reset_target(version).await?;

        debug!("Target reset and ready for debugging");

        Ok(())
    }

    /// Reset the target SWD device.  Performs the initialization routine.
    ///
    /// Unlike [`Self::reset_swd_target()`], this function tries both versions
    /// of the SWD reset sequence - V1 first, then V2 multi-drop then V2.
    ///
    /// This function has to test V2 multi-drop before V2.  A non multi-drop
    /// V2 target may be disabled by the way the V2 multi-drop test works.  On
    /// the other hand, multi-drop targets may response to the way the V2 non
    /// multi-drop test works.
    ///
    /// Returns:
    /// - `Ok(IdCode)`: if successful, sharing the the IDCODE of the target's
    ///   SWD implementation.
    /// - `Err(SwdError)`: if there was an error during the reset sequence, or
    ///   SwdError::NotReady if the target is not yet initialized.
    pub async fn reset_swd_target(&mut self) -> Result<(), SwdError> {
        debug!("Trying V1 reset sequence");
        if self.swd.reset_target(Version::V1).await.is_ok() {
            debug!("V1 reset sequence successful");
            return Ok(());
        }

        debug!("Trying V2 multi-drop reset sequence");
        if let Ok(targets) = self
            .swd
            .reset_detect_multidrop(&TARGET_SEL_RP2040_ALL)
            .await
        {
            // Guaranteed to be > 0 if OK
            assert!(
                !targets.is_empty(),
                "No targets found in multi-drop reset sequence"
            );
            let target = targets[0];

            // We don't want to connect to the RP2040's rescue DP.  If this is
            // the first one, the RP2040 probably needs a reboot.
            if target.target() == airfrog_core::arm::dp::TARGET_SEL_RP2040_RESCUE_DP {
                debug!("Found RP2040 rescue target - not resetting");
                return Err(SwdError::NotReady);
            }

            debug!("Found V2 multi-drop targets - connecting to {target}");
            if self.swd.reset_multidrop_target(&target).await.is_ok() {
                debug!("V2 multi-drop reset sequence successful");
                return Ok(());
            }
        }

        debug!("Trying V2 non multi-drop reset sequence");
        self.swd
            .reset_target(Version::V2)
            .await
            .inspect(|_| debug!("V2 reset sequence successful"))
            .inspect_err(|_| debug!("All reset sequences failed"))
    }

    /// Returns the IDCODE of the target device, if available.
    ///
    /// Returns:
    /// - `Some(IdCode)`: if the IDCODE is available, containing the IDCODE of
    ///   the target device.
    pub fn idcode(&self) -> Option<IdCode> {
        self.swd.idcode()
    }

    /// Returns the MCU details for the target device, if available.
    ///
    /// Returns:
    /// - `Some(Mcu)`: if the MCU details are available, containing the
    ///   MCU information.
    pub fn mcu(&self) -> Option<Mcu> {
        self.swd.mcu()
    }

    /// Checks if the MEM-AP is present and is of the expected type.
    ///
    /// Note that this function fails on the STM32F4, and puts the device into
    /// a state where it must be hard reset to recover.
    ///
    /// Returns:
    /// - `Ok(())`: if the MEM-AP is present and of the expected type.
    /// - `Err(SwdError)`: if the MEM-AP is not present or is of an unexpected
    ///   type.
    pub async fn check_mem_ap(&mut self) -> Result<(), SwdError> {
        let idr = self.swd.read_ap_register(IdrRegister).await?;
        info!("AP IDR: {} {}", idr, idr.idr_info());

        if idr.ap_type() != Idr::AP_TYPE_AMBA_AHB5 {
            warn!(
                "Expected AHB5 MEM-AP, got class 0x{:X}, type 0x{:X}",
                idr.class(),
                idr.ap_type()
            );
            return Err(SwdError::OperationFailed(format!("unexpected idr {idr}")));
        }

        Ok(())
    }

    /// Reads a 32-bit value from the target's memory at the specified address.
    ///
    /// This address can usually be RAM, flash, or any other memory-mapped
    /// location in the target's address space, such as peripheral registers.
    ///
    /// Arguments:
    /// - `addr`: The address in the target's memory to read from.
    ///
    /// Returns:
    /// - `Ok(u32)`: if the read was successful, containing the 32-bit value
    ///   read from the target's memory.
    /// - `Err(SwdError)`: if there was an error reading from the target's
    ///   memory, such as if the target did not respond, or if the address
    ///   value readback in the TAR register mismatched the expected value.
    pub async fn read_mem(&mut self, addr: u32) -> Result<u32, SwdError> {
        self.swd.read_mem(addr).await
    }

    /// Writes a 32-bit value to the target's memory at the specified address
    ///
    /// This address can usually be RAM, flash, or any other memory-mapped
    /// location in the target's address space, such as peripheral registers.
    ///
    /// Note that to write to flash, the MCU usually requires magic values be
    /// written to its flash register(s) before it can be programmed.  See
    /// [`Self::unlock_flash()`].
    ///
    /// Arguments:
    /// - `addr`: The address in the target's memory to write to.
    /// - `data`: The 32-bit value to write to the target's memory at the
    ///   specified address.
    ///
    /// Returns:
    /// - `Ok(())`: if the write was successful.
    /// - `Err(SwdError)`: if there was an error writing to the target's
    ///   memory, such as if the target did not respond, or if the address
    ///   value readback in the TAR register mismatched the expected value.
    pub async fn write_mem(&mut self, addr: u32, data: u32) -> Result<(), SwdError> {
        self.swd.write_mem(addr, data).await
    }

    /// Reads a block of memory from the target device.
    ///
    /// Arguments:
    /// - `addr`: The starting address in the target's memory to read from.
    /// - `buf`: A mutable slice to store the read data.  The length of this
    ///   slice determines how many words will be read.
    /// - `fast`: If true, avoids checking DP CTRL/STAT for errors until the
    ///   end of the read operation.  If false, checks for errors after each
    ///   read, and only returns data from non-errored reads.
    ///
    /// Returns:
    /// - `Ok(())`: if the read was successful, with `buf` containing the
    ///   read data.
    /// - `Err((SwdError, usize))`: if there was an error reading the memory,
    ///   where the `usize` is the number of successful reads before the error,
    ///   so the number of valid data values in `buf`.
    pub async fn read_mem_bulk(
        &mut self,
        addr: u32,
        buf: &mut [u32],
        fast: bool,
    ) -> Result<(), (SwdError, usize)> {
        self.swd.set_addr_inc(true).await.map_err(|e| (e, 0))?;
        match self.swd.read_mem_bulk(addr, buf, fast).await {
            Ok(()) => self
                .swd
                .set_addr_inc(false)
                .await
                .map_err(|e| (e, buf.len())),
            Err((e, count)) => {
                self.swd
                    .set_addr_inc(false)
                    .await
                    .map_err(|_| (e.clone(), count))?;
                Err((e, count))
            }
        }
    }

    /// Writes a block of memory to the target device.
    ///
    /// Arguments:
    /// - `addr`: The starting address in the target's memory to write to.
    /// - `buf`: A slice containing the data to write.  The length of this
    ///   slice determines how many words will be written.
    /// - `fast`: If true, avoids checking DP CTRL/STAT for errors until the
    ///   end of the write operation.  If false, checks for errors after each
    ///   write, and stops on the first error.
    ///
    /// Returns:
    /// - `Ok(())`: if the write was successful.
    /// - `Err((SwdError, usize))`: if there was an error writing the memory,
    ///   where the `usize` is the number of successful writes before the
    ///   error.
    pub async fn write_mem_bulk(
        &mut self,
        addr: u32,
        buf: &[u32],
        fast: bool,
    ) -> Result<(), (SwdError, usize)> {
        self.swd.set_addr_inc(true).await.map_err(|e| (e, 0))?;
        match self.swd.write_mem_bulk(addr, buf, fast).await {
            Ok(()) => self
                .swd
                .set_addr_inc(false)
                .await
                .map_err(|e| (e, buf.len())),
            Err((e, count)) => {
                self.swd
                    .set_addr_inc(false)
                    .await
                    .map_err(|_| (e.clone(), count))?;
                Err((e, count))
            }
        }
    }

    /// Unlocks the flash memory of the target device for writing/erasing.
    ///
    /// On the STM32F4 this function writes the following values, in sequence,
    /// to the device's FLASH_KEYR register:
    ///
    /// - 0x45670123 (KEY1)
    /// - 0xCDEF89AB (KEY2)
    ///
    /// The flash can be re-locked by setting the LOCK bit in the FLASH_CR
    /// register.
    ///
    /// Re-lock the flash using [`Self::lock_flash()`].
    ///
    /// Returns:
    /// - `Ok(())`: if the flash was successfully unlocked.
    /// - `Err(SwdError)`: if there was an error unlocking the flash, such as
    ///   if the target did not respond, or if the flash keys could not be
    ///   written correctly.
    pub async fn unlock_flash(&mut self) -> Result<(), SwdError> {
        let mcu = self.swd.mcu().ok_or(SwdError::NotReady)?;
        match mcu {
            Mcu::Stm32(stm) => match stm.mcu().family() {
                StmFamily::F4 => {
                    debug!("Unlocking STM32F4 flash");
                    self.write_mem(Stm32F4FlashKeyr::ADDRESS, Stm32F4FlashKeyr::KEY1)
                        .await?;
                    self.write_mem(Stm32F4FlashKeyr::ADDRESS, Stm32F4FlashKeyr::KEY2)
                        .await
                }
                _ => {
                    warn!("Unlocking flash for non-F4 STM32 is not supported");
                    Err(SwdError::Unsupported)
                }
            },
            _ => {
                warn!("Unlocking flash for non-ST MCU is not supported");
                Err(SwdError::Unsupported)
            }
        }
    }

    /// Locks the flash memory of the target device.
    ///
    /// On the STM32F4 this function sets the LOCK bit in the FLASH_CR register.
    ///
    /// Usually called after a [`Self::unlock_flash()`] operation once the
    /// flash has been programmed or erased.
    ///
    /// Returns:
    /// - `Ok(())`: if the flash was successfully locked.
    /// - `Err(SwdError)`: if there was an error locking the flash, such as if
    ///   the target did not respond, or if the FLASH_CR register could not be
    ///   written correctly.
    pub async fn lock_flash(&mut self) -> Result<(), SwdError> {
        let mcu = self.swd.mcu().ok_or(SwdError::NotReady)?;
        match mcu {
            Mcu::Stm32(stm) => match stm.mcu().family() {
                StmFamily::F4 => {
                    debug!("Locking STM32F4 flash");
                    let addr = Stm32F4FlashCr::ADDRESS;
                    let mut flash_cr = self.read_mem(addr).await?;
                    flash_cr |= 1 << Stm32F4FlashCr::LOCK_BIT;
                    self.write_mem(addr, flash_cr).await
                }
                _ => {
                    warn!("Locking flash for non-F4 STM32 is not supported");
                    Err(SwdError::Unsupported)
                }
            },
            _ => {
                warn!("Locking flash for non-ST MCU is not supported");
                Err(SwdError::Unsupported)
            }
        }
    }

    async fn execute_stm32f4_erase(&mut self, flash_cr: u32) -> Result<(), SwdError> {
        let cr_addr = Stm32F4FlashCr::ADDRESS;

        // Start the operation
        let flash_cr = flash_cr | (1 << Stm32F4FlashCr::STRT_BIT);
        self.write_mem(cr_addr, flash_cr).await?;

        // Poll for completion
        debug!("Flash erase operation started");
        let sr_addr = Stm32F4FlashSr::ADDRESS;
        let mut loop_counter = 0;
        loop {
            let flash_sr: Stm32F4FlashSr = self.read_mem(sr_addr).await?.into();
            if flash_sr.errors() {
                warn!("Flash erase operation failed with errors: {flash_sr}");
                return Err(SwdError::OperationFailed(format!(
                    "flash erase failure {flash_sr}"
                )));
            }
            if !flash_sr.busy() {
                break;
            }
            Timer::after(Duration::from_millis(1)).await;
            loop_counter += 1;
            if loop_counter % 1000 == 0 {
                debug!("... waiting for flash erase operation to complete");
            }
        }

        debug!("Flash erase operation completed successfully");
        Ok(())
    }

    /// Erase a flash memory sector
    ///
    /// This function erases a flash memory sector on the target device.
    ///
    /// Performing this operation on a running MCU is likely to lead to bad
    /// effects.  Care must be taken by the user to select the correct sector,
    /// which is MCU specific.
    ///
    /// Arguments:
    /// - `sector`: The sector number to erase.  This is MCU-specific, and
    ///   should be determined from the MCU's documentation.
    ///
    /// Returns:
    /// - `Ok(())`: if the sector was successfully erased.
    /// - `Err(SwdError)`: if there was an error erasing the sector, such as if
    ///   the target did not respond, or if the FLASH_CR register could not be
    ///   written correctly.
    pub async fn erase_sector(&mut self, sector: u32) -> Result<(), SwdError> {
        let mcu = self.swd.mcu().ok_or(SwdError::NotReady)?;
        match mcu {
            Mcu::Stm32(stm) => match stm.mcu().family() {
                StmFamily::F4 => {
                    debug!("Erasing STM32F4 flash sector {sector}");
                    let addr = Stm32F4FlashCr::ADDRESS;
                    let mut flash_cr = self.read_mem(addr).await?;

                    // Set the sector erase operation
                    flash_cr |= 1 << Stm32F4FlashCr::SER_BIT;
                    flash_cr |= (sector & Stm32F4FlashCr::SNB_MASK) << Stm32F4FlashCr::SNB_SHIFT;
                    flash_cr |= Stm32F4FlashCr::PSIZE_X64 << Stm32F4FlashCr::PSIZE_SHIFT;
                    self.write_mem(addr, flash_cr).await?;

                    // Perform the erase and wait for completion
                    self.execute_stm32f4_erase(flash_cr).await
                }
                _ => {
                    warn!("Erasing flash for non-F4 STM32 is not supported");
                    Err(SwdError::Unsupported)
                }
            },
            _ => {
                warn!("Erasing flash for non-ST MCU is not supported");
                Err(SwdError::Unsupported)
            }
        }
    }

    /// Erases all flash sectors on the target device.
    ///
    /// Returns:
    /// - `Ok(())`: if all flash sectors were successfully erased.
    /// - `Err(SwdError)`: if there was an error erasing the sectors, such as
    ///   if the target did not respond, or if the FLASH_CR register could not
    ///   be written correctly.
    pub async fn erase_all(&mut self) -> Result<(), SwdError> {
        let mcu = self.swd.mcu().ok_or(SwdError::NotReady)?;
        match mcu {
            Mcu::Stm32(stm) => match stm.mcu().family() {
                StmFamily::F4 => {
                    debug!("Erasing all STM32F4 flash sectors");
                    let addr = Stm32F4FlashCr::ADDRESS;
                    let mut flash_cr = self.read_mem(addr).await?;

                    // Set the mass erase operation
                    flash_cr |= 1 << Stm32F4FlashCr::MER_BIT;
                    self.write_mem(addr, flash_cr).await?;

                    // Perform the erase and wait for completion
                    self.execute_stm32f4_erase(flash_cr).await
                }
                _ => {
                    warn!("Erasing flash for non-F4 STM32 is not supported");
                    Err(SwdError::Unsupported)
                }
            },
            _ => {
                warn!("Erasing flash for non-ST MCU is not supported");
                Err(SwdError::Unsupported)
            }
        }
    }

    async fn execute_stm32f4_program(&mut self, addr: u32, flash_cr: u32) -> Result<(), SwdError> {
        let cr_addr = Stm32F4FlashCr::ADDRESS;

        // Set PG bit to enable programming
        let flash_cr = flash_cr | (1 << Stm32F4FlashCr::PG_BIT);
        self.write_mem(cr_addr, flash_cr).await?;

        // Poll for completion
        let sr_addr = Stm32F4FlashSr::ADDRESS;
        loop {
            let flash_sr: Stm32F4FlashSr = self.read_mem(sr_addr).await?.into();
            if flash_sr.errors() {
                warn!("Flash program operation failed with errors: {flash_sr}");
                return Err(SwdError::OperationFailed(format!(
                    "flash program failure {flash_sr}"
                )));
            }
            if !flash_sr.busy() {
                break;
            }
            Timer::after(Duration::from_millis(1)).await;
        }

        debug!("Flash program operation completed at 0x{addr:08X}");
        Ok(())
    }

    // Implements the core write flash logic
    async fn write_flash(
        &mut self,
        addr: u32,
        data: u32,
        bytes: OperationBytes,
    ) -> Result<(), SwdError> {
        let mcu = self.swd.mcu().ok_or(SwdError::NotReady)?;
        match mcu {
            Mcu::Stm32(stm) => match stm.mcu().family() {
                StmFamily::F4 => {
                    // Perform byte alignment checking as well as getting the
                    // correct programming size
                    let psize = match bytes {
                        OperationBytes::BYTE1 => Stm32F4FlashCr::PSIZE_X8,
                        OperationBytes::BYTE2 => {
                            if addr & 1 != 0 {
                                warn!("Address 0x{addr:08X} is not half-word aligned");
                                return Err(SwdError::Api);
                            }
                            Stm32F4FlashCr::PSIZE_X16
                        }
                        OperationBytes::BYTE4 => {
                            if addr & 3 != 0 {
                                warn!("Address 0x{addr:08X} is not word aligned");
                                return Err(SwdError::Api);
                            }
                            Stm32F4FlashCr::PSIZE_X32
                        }
                    };

                    let cr_addr = Stm32F4FlashCr::ADDRESS;
                    let mut flash_cr = self.read_mem(cr_addr).await?;

                    // Set PSIZE
                    flash_cr &= !(Stm32F4FlashCr::PSIZE_MASK << Stm32F4FlashCr::PSIZE_SHIFT);
                    flash_cr |= psize << Stm32F4FlashCr::PSIZE_SHIFT;

                    // Write the data directly to target address
                    self.write_mem(addr, data).await?;

                    self.execute_stm32f4_program(addr, flash_cr).await
                }
                _ => Err(SwdError::Unsupported),
            },
            _ => {
                warn!("Writing flash for non-ST MCU is not supported");
                Err(SwdError::Unsupported)
            }
        }
    }

    /// Writes a byte to flash memory on the target device.
    ///
    /// The flash must be unlocked using [`Self::unlock_flash()`] before
    /// calling this function, and the target address must be erased (0xFF)
    /// before programming.
    ///
    /// Arguments:
    /// - `addr`: The address in the target's flash memory to write the byte
    ///   to.
    /// - `data`: The byte value to write to flash.
    ///
    /// Returns:
    /// - `Ok(())`: if the flash write was successful.
    /// - `Err(SwdError)`: if there was an error writing to flash, such as if
    ///   the target did not respond, if the flash controller reported errors,
    ///   or if the MCU family is not supported.
    pub async fn write_flash_u8(&mut self, addr: u32, data: u8) -> Result<(), SwdError> {
        self.write_flash(addr, data as u32, OperationBytes::BYTE1)
            .await
    }

    /// Writes a half-word to flash memory on the target device.
    ///
    /// The flash must be unlocked using [`Self::unlock_flash()`] before
    /// calling this function, and the target address must be erased (0xFF)
    /// before programming.
    ///
    /// Arguments:
    /// - `addr`: The address in the target's flash memory to write the
    ///   half-word to.  Must be half-word aligned (even address).
    /// - `data`: The half-word value to write to flash.
    ///
    /// Returns:
    /// - `Ok(())`: if the flash write was successful.
    /// - `Err(SwdError)`: if there was an error writing to flash, such as if
    ///   the target did not respond, if the flash controller reported errors,
    ///   or if the MCU family is not supported.
    pub async fn write_flash_u16(&mut self, addr: u32, data: u16) -> Result<(), SwdError> {
        if addr & 1 != 0 {
            warn!("Address 0x{addr:08X} is not half-word aligned");
            return Err(SwdError::Api);
        }
        self.write_flash(addr, data as u32, OperationBytes::BYTE2)
            .await
    }

    /// Writes a word to flash memory on the target device.
    ///
    /// The flash must be unlocked using [`Self::unlock_flash()`] before
    /// calling this function, and the target address must be erased (0xFF)
    /// before programming.
    ///
    /// Arguments:
    /// - `addr`: The address in the target's flash memory to write the word
    ///   to. Must be word aligned (address divisible by 4).
    /// - `data`: The word value to write to flash.
    ///
    /// Returns:
    /// - `Ok(())`: if the flash write was successful.
    /// - `Err(SwdError)`: if there was an error writing to flash, such as if
    ///   the target did not respond, if the flash controller reported errors,
    ///   or if the MCU family is not supported.
    pub async fn write_flash_u32(&mut self, addr: u32, data: u32) -> Result<(), SwdError> {
        self.write_flash(addr, data, OperationBytes::BYTE4).await
    }

    /// Writes a block of words to flash memory on the target device.
    ///
    /// This function writes multiple 32-bit words to consecutive flash
    /// addresses. The flash must be unlocked using [`Self::unlock_flash()`]
    /// before calling this function, and all target addresses must be erased
    /// (0xFF) before programming.
    ///
    /// The function sets the flash controller to word programming mode and
    /// writes each word individually, polling for completion after each write
    /// operation.
    ///
    /// Arguments:
    /// - `addr`: The starting address in the target's flash memory to write
    ///   to. Must be word aligned (address divisible by 4).
    /// - `data`: A slice containing the words to write to flash.
    ///
    /// Returns:
    /// - `Ok(())`: if all flash writes were successful.
    /// - `Err(SwdError)`: if there was an error writing to flash, such as if
    ///   the target did not respond, if the flash controller reported errors
    ///   during any write operation, or if the MCU family is not supported.
    ///
    /// TODO - return number of words actually written
    pub async fn write_flash_bulk(&mut self, addr: u32, data: &[u32]) -> Result<(), SwdError> {
        let mcu = self.swd.mcu().ok_or(SwdError::NotReady)?;
        match mcu {
            Mcu::Stm32(stm) => match stm.mcu().family() {
                StmFamily::F4 => {
                    self.swd.set_addr_inc(true).await?;

                    let cr_addr = Stm32F4FlashCr::ADDRESS;
                    let mut flash_cr = self.read_mem(cr_addr).await?;

                    // Set PSIZE for word programming (10)
                    flash_cr &= !(0b11 << Stm32F4FlashCr::PSIZE_SHIFT);
                    flash_cr |= 0b10 << Stm32F4FlashCr::PSIZE_SHIFT;

                    // Set PG bit
                    flash_cr |= 1 << Stm32F4FlashCr::PG_BIT;
                    self.write_mem(cr_addr, flash_cr).await?;

                    // Write each word and wait for completion
                    for (i, &word) in data.iter().enumerate() {
                        let target_addr = addr + (i as u32 * 4);
                        self.write_mem(target_addr, word).await?;

                        // Wait for this word to complete
                        let sr_addr = Stm32F4FlashSr::ADDRESS;
                        loop {
                            let flash_sr: Stm32F4FlashSr = self.read_mem(sr_addr).await?.into();
                            if flash_sr.errors() {
                                warn!(
                                    "Flash program operation failed at 0x{target_addr:08X}: {flash_sr}"
                                );
                                self.swd
                                    .set_addr_inc(false)
                                    .await
                                    .inspect_err(|e| warn!("Failed to reset address increment after flash program failure: {e}"))
                                    .ok();
                                return Err(SwdError::OperationFailed(format!(
                                    "flash program failure {flash_sr}"
                                )));
                            }
                            if !flash_sr.busy() {
                                break;
                            }
                            Timer::after(Duration::from_millis(1)).await;
                        }
                    }

                    debug!("Flash bulk program completed, {} words written", data.len());
                    self.swd.set_addr_inc(false).await
                }
                _ => Err(SwdError::Unsupported),
            },
            _ => {
                warn!("Writing flash for non-ST MCU is not supported");
                Err(SwdError::Unsupported)
            }
        }
    }
}
