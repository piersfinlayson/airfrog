// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! ARM SWD Interface
//!
//! This module implements the SWD interface for communicating with ARM
//! devices.  It provides `SwdInterface` for performing SWD operations, and
//! `SwdOp` for creating low-level SWD operations.

use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt;
use embassy_time::{Duration, Timer};
use esp_hal::gpio::{InputPin, OutputPin};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use airfrog_core::Mcu;
use airfrog_core::arm::Cortex;
use airfrog_core::arm::ap::{IDR_AHB_AP_KNOWN, Idr, IdrRegister};
use airfrog_core::arm::dp::{Abort, CtrlStat, IdCode, RdBuff, Select};
use airfrog_core::arm::dp::{
    AbortRegister, CtrlStatRegister, IdCodeRegister, RdBuffRegister, SelectRegister, TargetSel,
    TargetSelRegister,
};
use airfrog_core::arm::map::{Csw, CswRegister, Drw, DrwRegister, Tar, TarRegister};
use airfrog_core::arm::register::{
    ApRegister, DpRegister, ReadableRegister, RegisterDescriptor, WritableRegister,
};
use airfrog_core::rp;
use airfrog_core::rp::RpDetails;
use airfrog_core::stm::{StmDetails, StmDeviceId, StmFlashSize, StmUniqueId};

use crate::SwdError;
use crate::protocol::{
    LineState, POST_SINGLE_OPERATION_CYCLES, Speed, SwdProtocol, Version, calculate_parity,
};

#[doc(inline)]
pub use crate::debug::DebugInterface;

// SWD wraps read/writes using auto-incrementing at a 1K boundary, although
// this is implementation dependent.
const SWD_MEMORY_BOUNDARY: u32 = 0x400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiDropTarget {
    target: TargetSel,
    idcode: IdCode,
}

impl MultiDropTarget {
    pub const fn new(target: TargetSel, idcode: IdCode) -> Self {
        MultiDropTarget { idcode, target }
    }

    pub fn target(&self) -> TargetSel {
        self.target
    }

    pub fn idcode(&self) -> IdCode {
        self.idcode
    }

    pub fn name(&self) -> &'static str {
        match self.target.data() {
            0x01002927 => "RP2040 Core 0",
            0x11002927 => "RP2040 Core 1",
            0xF1002927 => "RP2040 Rescue DP",
            _ => {
                if self.target.data() & 0x1002927 == 0x1002927 {
                    "RP2040 Core custom instance ID"
                } else {
                    "Unknown"
                }
            }
        }
    }
}

impl fmt::Display for MultiDropTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}/{})", self.name(), self.target, self.idcode)
    }
}

// Default retries after a Wait ACK
const DEFAULT_WAIT_RETRIES: u32 = 2;

/// SWD Interface object
///
/// This is used by [`DebugInterface`] to perform individual SWD operations on
/// the target.  It exposes a low-level interface to perform SWD operations.
/// Most applications will prefer to use [`DebugInterface`], which provides a
/// higher-level interface for common groups of SWD operations.
///
/// Create using `SwdInterface::new()` passing in an [`SwdProtocol`] instance.
///
/// ```rust
/// use airfrog_swd::SwdProtocol;
///
/// let peripherals = esp_hal::init(config);
/// let swdio_pin = peripherals.GPIO0;
/// let swclk_pin = peripherals.GPIO1;
/// let swd = SwdProtocol::new(swdio_pin, swclk_pin);
///
/// let mut swd_if = SwdInterface::new(swd);
///
/// swd_if.reset_target().await.unwrap();
/// esp_println::println!("IDCODE: {}", swd_if.idcode().unwrap());
/// ```
///
/// Or, create using `SwdInterface::from_pins`:
///
/// ```rust
/// use airfrog_swd::SwdInterface;
///
/// let peripherals = esp_hal::init(config);
/// let swdio_pin = peripherals.GPIO0;
/// let swclk_pin = peripherals.GPIO1;
/// let mut swd_if = SwdInterface::from_pins(swdio_pin, swclk_pin);
///
/// swd_if.reset_target().await.ok();
/// esp_println::println!("IDCODE: {}", swd_if.idcode().unwrap());
/// ```
///
#[derive(Debug)]
pub struct SwdInterface<'a> {
    protocol: SwdProtocol<'a>,
    idcode: Option<IdCode>,
    mcu: Option<Mcu>,
    idr: Option<Idr>,
    powered_up: bool,
    dp_select: Select,
    addr_inc: bool,
    wait_retries: u32,
    check_power: bool,

    // Version is updated when IDCODE is read after reset.  If the target is
    // reset without reading the IDCODE, version is not set.
    reset_version: Option<Version>,
}

impl<'a> SwdInterface<'a> {
    // Resets internal state of the SWD interface.
    fn reset_internal_state(&mut self) {
        self.idcode = None;
        self.mcu = None;
        self.idr = None;
        self.powered_up = false;
        self.dp_select = Select::default();
        self.addr_inc = false;
        self.check_power = true;
        self.reset_version = None;
    }

    /// Creates a new SWD interface using the given [`SwdProtocol`] instance.
    ///
    /// It may be preferable to use [`SwdInterface::from_pins`] rather than
    /// this function, to avoid having to create the [`SwdProtocol`] instance
    /// manually.
    ///
    /// Arguments:
    /// - `protocol`: The [`SwdProtocol`] instance to use for SWD communication.
    ///
    /// Returns:
    /// - A new [`SwdInterface`] instance that uses the given protocol for SWD
    ///   communication.
    pub fn new(protocol: SwdProtocol<'a>) -> Self {
        Self {
            protocol,
            idcode: None,
            mcu: None,
            idr: None,
            powered_up: false,
            dp_select: Select::default(),
            addr_inc: false,
            wait_retries: DEFAULT_WAIT_RETRIES,
            check_power: true,
            reset_version: None,
        }
    }

    /// Creates a new SWD interface from the given pins.
    ///
    /// Arguments:
    /// - `swdio_pin`: The pin to use for SWDIO, which must implement both
    ///   `InputPin` and `OutputPin` traits.
    /// - `swclk_pin`: The pin to use for SWCLK, which must implement the
    ///   `OutputPin` trait.
    ///
    /// Returns:
    /// - A new [`SwdInterface`] instance that uses the given pins for SWD
    ///   communication.
    pub fn from_pins(
        swdio_pin: impl InputPin + OutputPin + 'a,
        swclk_pin: impl OutputPin + 'a,
    ) -> Self {
        let swd = SwdProtocol::new(swdio_pin, swclk_pin);
        Self::new(swd)
    }

    /// Sets the SWD speed for this interface.
    ///
    /// Can be changed at any time.  For example, if [`Self::reset_target()`]
    /// fails, retry with a slow speed.
    ///
    /// Arguments:
    /// - `speed`: The new speed to set for the SWD interface.
    pub fn set_swd_speed(&mut self, speed: Speed) {
        trace!("Exec:  Set {speed:?}");
        self.protocol.set_speed(speed);
    }

    /// Gets the SWD speed for this interface.
    ///
    /// Returns:
    /// - The current speed of the SWD interface.
    pub fn swd_speed(&self) -> Speed {
        self.protocol.speed()
    }

    /// Returns whether the SWD interface is currently connected to a target.
    ///
    /// Returns:
    /// - `true` if the interface is connected to a target, `false` otherwise.
    pub fn is_connected(&self) -> bool {
        self.idcode.is_some()
    }

    /// If calling [`Self::reset_sequence_v1()`] or [`Self::reset_sequence_v2()`]
    /// directly, call this function afterwards to connect to the target.
    ///
    /// This
    /// - clears any errors on the ABORT register
    /// - reads RDBUFF and discards the value
    /// - powers up the debug domain
    /// - configures the MEM-AP
    /// - attempts to retrieve the MCU details.
    ///
    /// Arguments:
    /// - The DPIDR IDCODE of the target device
    ///
    /// Returns:
    /// - `Ok(Some(Mcu))`: if the target was successfully enabled, returning
    ///   the MCU details.
    /// - `Ok(None)`: if the target was successfully enabled, but no MCU
    ///   details were retrieved.
    /// - `Err(SwdError)`: if there was an error during the enabling process.
    pub async fn enable_target(&mut self, idcode: IdCode) -> Result<Option<Mcu>, SwdError> {
        // Send an ABORT, clearing any previous errors
        trace!("Exec:  Clear ABORT");
        self.clear_errors().await?;

        // Read the RDBUFF register to clear it
        trace!("Exec:  Read RDBUFF");
        let _ = self.read_rd_buff_fast(false).await?;

        // Power up the debug device
        trace!("Exec:  Power up debug domain");
        self.power_up_debug_domain().await?;

        // Configure the MEM-AP
        trace!("Exec:  Configure MEM-AP");
        self.configure_mem_ap().await?;

        // Get MCU details
        let mcu = self.get_mcu(idcode).await.map(Some)?;
        debug!("Value: {mcu:?}");

        Ok(mcu)
    }

    /// Resets and connects to the target's SWD interface.
    ///
    /// This performs the standard SWD reset sequence, for either V1 or V2
    /// targets, and then performs the necessary steps to connect to the
    /// target's SWD interface.
    ///
    /// See [`Self::reset_multidrop_target()`] for connecting to v2 multi-drop
    /// targets.
    ///
    /// Returns:
    /// - `Ok()`: if successful.
    /// - `Err(SwdError)`: if there was an error during the initialization
    ///   sequence.
    pub async fn reset_target(&mut self, version: Version) -> Result<(), SwdError> {
        trace!("Exec:  Reset and enable SWD");
        // Attempt to connect to the device over SWD.
        let idcode = match version {
            Version::V1 => self.reset_sequence_v1().await?,
            Version::V2 => self.reset_sequence_v2(false, true).await?.unwrap(),
        };

        // Enable the target, and get the MCU details if possible.  (It may
        // not be if we don't know about this type of MCU.)
        let mcu = self
            .enable_target(idcode)
            .await
            .inspect_err(|_| self.reset_version = None)?;

        // Store off the target details
        self.mcu = mcu;
        self.idcode = Some(idcode);

        Ok(())
    }

    /// Call to perform a SWD line reset, but **does not** fully connect to the
    /// target.  Use [`Self::reset_target()`] to perform the entire sequence.
    /// Use this function when you want more control over the connection
    /// process.
    ///
    /// Must be called before any other operations on the target, and must also
    /// be called if any permanent failures occur while communicating with the
    /// target.
    ///
    /// Returns:
    /// - `Ok(IdCode)` if the reset sequence was successful and the IDCODE
    ///   was read from the target.
    /// - `Err(SwdError)` if there was an error performing the reset sequence,
    ///   or if the IDCODE could not be read from the target.
    ///
    /// Once this sequence has completed successfully, and an [`IdCode`] is
    /// returned, other operations can be performed on the target.
    pub async fn reset_sequence_v1(&mut self) -> Result<IdCode, SwdError> {
        trace!("Exec:  Reset SWD");
        self.reset_internal_state();

        // Start off with a known state and a brief pause
        self.protocol.reset_prep().await;

        // Do 50 clocks before SWD to dormat seq (no low clocks)
        self.protocol.pre_line_reset();
        Timer::after(Duration::from_micros(100)).await;

        // Do the SWD to dormant sequence.  Ensures any v2 targets are properly
        // reset.
        self.protocol.swd_to_dormant_sequence();
        Timer::after(Duration::from_micros(100)).await;

        // Do the pre JTAG-to-SWD sequence reset (no low clocks)
        self.protocol.pre_line_reset();
        Timer::after(Duration::from_micros(100)).await;

        self.protocol.jtag_to_swd_sequence();
        Timer::after(Duration::from_micros(100)).await;

        self.protocol.line_reset_after().await;

        // Write 0xFFFFFFFF to TARGETSEL to ensure any previously selected
        // multi-drop target is deselected.  This should be benign on a non-
        // multi-drop target, but prevents us from getting a response when a
        // multi-drop target is being used.  Of course, multi-drop is a v2
        // feature, and this is a v1 reset sequence - but previously selected
        // v2 multi-drop targets remain selected across the v1 reset sequence.
        //
        // This may also prevent v1 targes responding.  If so, they'll respond
        // in the v2 sequence with disable_md_targets set to false.
        self.do_write_target_sel(TargetSel::new(0xFFFFFFFF)).await?;

        // Read IDCODE to confirm SWD is now running
        let idcode = self.read_idcode().await?;
        trace!("Value: IDCODE: {idcode}");
        self.reset_version = Some(Version::V1);
        Ok(idcode)
    }

    /// Performs the SWD reset sequence to exit dormant state, for SWD v2.
    ///
    /// This sequence is used to exit dormant state on targets that support
    /// SWD v2, such as the RP2040/Pico and RP2350/Pico 2.
    ///
    /// This function does not read the IDCODE or, in the case of multi-drop
    /// targets (RP2040/Pico), select the target using TARGETSEL.
    ///
    /// Arguments:
    /// - `disable_targets`: If `true`, disables multidrop targets before
    ///   reading the ID code and/or exiting
    /// - `get_idcode`: If `true`, the IDCODE will be read after the
    ///   dormant exit sequence.  If `false`, the IDCODE will not be read.
    ///
    /// Returns:
    /// - `Ok(Some(IdCode))`: if the dormant exit was successful and the
    ///   IDCODE was (if `get_idcode` is `true`).
    /// - `Ok(None)`: if the dormant exit was successful but the IDCODE was
    ///   not read (if `get_idcode` is `false`).
    /// - `Err(SwdError)`: if there was an error reading the IDCODE register
    ///   (if `get_idcode` is `true`).
    pub async fn reset_sequence_v2(
        &mut self,
        disable_md_targets: bool,
        get_idcode: bool,
    ) -> Result<Option<IdCode>, SwdError> {
        trace!("Exec:  Reset SWD");
        self.reset_internal_state();

        // Start off with a known state and a brief pause
        self.protocol.reset_prep().await;

        // Do 50 clocks before SWD to dormat seq (no low clocks)
        self.protocol.pre_line_reset();
        Timer::after(Duration::from_micros(100)).await;

        // Do the SWD to dormant sequence.  Ensures any v2 targets are properly
        // reset.
        self.protocol.swd_to_dormant_sequence();
        Timer::after(Duration::from_micros(100)).await;

        // Do the pre JTAG-to-SWD sequence reset (no low clocks)
        self.protocol.pre_line_reset();
        Timer::after(Duration::from_micros(100)).await;

        // Do the pre select alert sequence
        self.protocol.pre_sel_alert_seq();
        self.protocol.sel_alert_seq();
        self.protocol.post_sel_alert_seq();
        self.protocol.swd_act_code();

        // Line reset
        self.protocol.line_reset_after().await;

        if disable_md_targets {
            self.do_write_target_sel(TargetSel::new(0xFFFFFFFF)).await?;
        }

        if get_idcode {
            // Read IDCODE to confirm SWD is now running
            let idcode = self.read_idcode().await?;
            trace!("Value: IDCODE after dormant exit: {idcode}");
            self.reset_version = Some(Version::V2);
            Ok(Some(idcode))
        } else {
            Ok(None)
        }
    }

    /// Performs the SWD reset sequence to exit dormant state, for SWD v2,
    /// and then detects multi-drop targets based on those provided.
    ///
    /// Arguments:
    /// - `targets`: A slice of [`TargetSel`] instances representing the
    ///   targets to detect.
    ///
    /// Returns:
    /// - `Ok(Vec<MultiDropTarget>)`: A vector of detected multi-drop targets.
    ///   If OK is returned, guaranteed not to be empty.
    /// - `Err(SwdError)`: If there was an error during the detection
    ///   sequence.
    pub async fn reset_detect_multidrop(
        &mut self,
        targets: &[TargetSel],
    ) -> Result<Vec<MultiDropTarget>, SwdError> {
        trace!("Exec:  Reset and detect multi-drop targets");

        // Reset the SWD interface - we always use SWD v2 reset sequence when
        // attempting to detect multi-drop targets.  However, we don't attempt
        // to retrieve the IDCODE.
        let _ = self.reset_sequence_v2(true, false).await;

        // Try and detect each target in turn
        let mut found_md_targets = Vec::new();
        for target in targets {
            // Line reset
            self.protocol.line_reset_after().await;

            // Write this target to TARGETSEL
            self.do_write_target_sel(*target).await?;

            // Try and read IDCODE (DPIDR)
            if let Ok(idcode) = self.read_idcode().await {
                let md_target = MultiDropTarget {
                    target: *target,
                    idcode,
                };
                debug!("Value: Found target {target} IDCODE {idcode}");
                found_md_targets.push(md_target);
            } else {
                trace!("Info:  Target {target} not found");
            }
        }

        if found_md_targets.is_empty() {
            return Err(SwdError::OperationFailed(
                "no multi-drop targets detected".to_string(),
            ));
        }

        Ok(found_md_targets)
    }

    /// Used to reset SWD (v2) and connect to a specific multi-drop target.
    ///
    /// [`Self::reset_detect_multidrop()`] should be used to detect available
    /// multidrop targets, before calling this function.
    ///
    /// Arguments:
    /// - `target`: The [`MultiDropTarget`] to reset and connect to.
    ///
    /// Returns:
    /// - `Ok(())`: if the reset and connection to the target was successful.
    /// - `Err(SwdError)`: if there was an error resetting the target.
    pub async fn reset_multidrop_target(
        &mut self,
        target: &MultiDropTarget,
    ) -> Result<(), SwdError> {
        trace!("Exec:  Reset multi-drop target {}", target.name());

        // Reset the SWD interface - we always use SWD v2 reset sequence when
        // attempting to reset a multi-drop target.
        let _ = self.reset_sequence_v2(true, false).await?;

        // The reset sequence did a 0xFFFFFFFF after the line reset.  This
        // ensures we didn't detect multi-drop targets then.  However, we now
        // need a line reset to get all the multi-drop targets listening again.
        self.protocol.line_reset_after().await;

        // Now write TARGETSEL
        self.do_write_target_sel(target.target()).await?;

        // Next, read the IDCODE
        let idcode = self.read_idcode().await?;
        trace!("Value: IDCODE: {idcode}");

        // Enable the target, and get the MCU details if possible.  (It may
        // not be if we don't know about this type of MCU.)
        self.reset_version = Some(Version::V2);
        let mcu = self
            .enable_target(idcode)
            .await
            .inspect_err(|_| self.reset_version = None)?;

        // Store off the target details
        self.mcu = mcu;
        self.idcode = Some(idcode);

        Ok(())
    }

    /// Function used to both check the target is initialized, and retrieve
    /// the version of the interface, in case the caller needs it.
    ///
    /// Returns:
    /// - `Ok(Version)`: if the interface is initialized and the version is
    ///   available.
    /// - `Err(SwdError::NotReady)`: if the interface is not initialized or
    ///   the version is not available.
    pub fn check_version(&self) -> Result<Version, SwdError> {
        if let Some(mcu) = &self.mcu {
            match mcu {
                Mcu::Stm32(_) => Ok(Version::V1),
                Mcu::Rp(_) => Ok(Version::V2),
                Mcu::Unknown(_) => self.reset_version.ok_or(SwdError::NotReady),
            }
        } else if let Some(version) = self.reset_version {
            Ok(version)
        } else {
            debug!("Attempt to perform SWD action before initialization/reset");
            Err(SwdError::NotReady)
        }
    }

    /// Performs a SWD operation to read the IDCODE register.
    ///
    /// Returns:
    /// - `Ok(IdCode)`: if the IDCODE was read successfully.
    /// - `Err(SwdError)`: if there was an error reading the IDCODE register.
    pub async fn read_idcode(&mut self) -> Result<IdCode, SwdError> {
        // IDCODE register never needs DP SELECT update, so read it directly
        let op = SwdOp::DpRead(IdCodeRegister::ADDRESS);
        let idcode = self.do_read_op(op, true).await?;

        Ok(idcode.into())
    }

    /// Performs a keepalive on the SWD interface.
    ///
    /// Queries the IDCODE register.
    ///
    /// Returns:
    /// - `Ok()`: if the keepalive was successful and the IDCODE was read.
    /// - `Err(SwdError)`: if there was an error reading the IDCODE register.
    pub async fn keepalive(&mut self) -> Result<(), SwdError> {
        let _version = self.check_version();

        // Clear out internal state if disconnected
        self.read_idcode()
            .await
            .map(|_| ())
            .inspect_err(|_| self.reset_internal_state())
    }

    /// Write a Debug Port register
    ///
    /// This function automatically handles setting the DP SELECT register if
    /// it is required.
    ///
    /// Arguments:
    /// - `reg`: The register to write, which must implement the `DpRegister`
    ///   trait.
    ///
    /// Returns:
    /// - `Ok(())` if the register was written successfully.
    /// - `Err(SwdError)` if there was an error writing the register.
    ///
    /// ```rust
    /// use airfrog_core::arm::dp::{Abort, AbortRegister};
    /// let abort_value = Abort(0)
    ///     .set_stkerrclr(true)
    ///     .set_stkcmpclr(true)
    ///     .set_wderrclr(true)
    ///     .set_orunerrclr(true);
    /// swd_if.write_dp_register(AbortRegister, abort_value).await?;
    /// ```
    pub async fn write_dp_register<R>(&mut self, _reg: R, value: R::Value) -> Result<(), SwdError>
    where
        R: WritableRegister + DpRegister,
        u32: From<R::Value>,
    {
        let _version = self.check_version();

        let op = SwdOp::DpWrite(R::ADDRESS);
        let raw_data = R::to_raw(value);

        self.write_operation(op, raw_data, true).await
    }

    /// Write an Access Port register
    ///
    /// This function automatically handles setting the DP SELECT register if
    /// it is required.
    ///
    /// Arguments:
    /// - `reg`: The register to write, which must implement the `ApRegister`
    ///   trait.
    ///
    /// Returns:
    /// - `Ok(())` if the register was written successfully.
    /// - `Err(SwdError)` if there was an error writing the register.
    ///
    /// ```rust
    /// use airfrog_core::arm::map::{Tar. TarRegister};
    /// let tar_value = Tar(0x2000_0000);
    /// swd_if.write_ap_register(TarRegister, tar_value).await?;
    /// ```
    pub async fn write_ap_register<R>(&mut self, _reg: R, value: R::Value) -> Result<(), SwdError>
    where
        R: WritableRegister + ApRegister,
        u32: From<R::Value>,
    {
        let _version = self.check_version();

        let op = SwdOp::ApWrite(R::ADDRESS);
        let raw_data = R::to_raw(value);

        self.write_operation(op, raw_data, true).await
    }

    /// Read a Debug Port register.
    ///
    /// This function automatically handles setting the DP SELECT register if
    /// it is required.
    ///
    /// Arguments:
    /// - `reg`: The register to read, which must implement the `DpRegister`
    ///   trait.
    ///
    /// Returns:
    /// - `Ok(value)` if the register was read successfully, where `value` is
    ///   the value read from the register.
    /// - `Err(SwdError)` if there was an error reading the register.
    ///
    /// ```rust
    /// use airfrog_core::arm::dp::CtrlStatRegister;
    /// let value = swd_if.read_dp_register(CtrlStatRegister).await?;
    /// esp_println::println!("DP CTRL/STAT value: {value}");
    /// ```
    pub async fn read_dp_register<R>(&mut self, _reg: R) -> Result<R::Value, SwdError>
    where
        R: ReadableRegister + DpRegister,
        R::Value: From<u32>,
    {
        let _version = self.check_version();

        let op = SwdOp::DpRead(R::ADDRESS);
        let raw_data = self.read_operation(op, true).await?;

        Ok(R::from_raw(raw_data))
    }

    /// Read an Access Port register
    ///
    /// This function automatically handles setting the DP SELECT register if
    /// it is required.  It also reads the AP read result from the DP RDBUFF
    /// register automatically.
    ///
    /// Arguments:
    /// - `reg`: The register to read, which must implement the `ApRegister`
    ///   trait.
    ///
    /// Returns:
    /// - `Ok(value)` if the register was read successfully, where `value` is
    ///   the value read from the RDBUFF register.
    /// - `Err(SwdError)` if there was an error reading the register.
    ///
    /// ```rust
    /// use airfrog_core::arm::map::DrwRegister;
    /// let value = swd_if.read_dp_register(DrwRegister).await?;
    /// esp_println::println!("AP CRW value: {value}");
    /// ```
    pub async fn read_ap_register<R>(&mut self, _reg: R) -> Result<R::Value, SwdError>
    where
        R: ReadableRegister + ApRegister,
        R::Value: From<u32>,
    {
        let _version = self.check_version();

        let op = SwdOp::ApRead(R::ADDRESS);
        let raw_data = self.read_operation(op, true).await?;
        Ok(R::from_raw(raw_data))
    }

    /// Call to update the DP SELECT register.
    ///
    /// The DP SELECT register is used to select the active debug port, and the
    /// active DP/AP register banks.
    ///
    /// It is unnecessary to call this function directly when writing DP and AP
    /// registers using `write_dp_register` and `write_ap_register`, as those
    /// functions will automatically update the DP SELECT register if is
    /// required.
    ///
    /// Arguments:
    /// - `select`: The new DP SELECT register value to set.
    ///
    /// Returns:
    /// - `Ok(())` if the DP SELECT register was updated successfully.
    /// - `Err(SwdError)` if there was an error writing to the DP SELECT register.
    pub async fn update_dp_select(&mut self, select: Select) -> Result<(), SwdError> {
        let _version = self.check_version();

        self.do_write_op(SwdOp::DpWrite(SelectRegister::ADDRESS), select.into(), true)
            .await?;

        // Check for errors after writing
        self.check_dp_errors(false).await?;

        // Success - update the internal state
        self.dp_select = select;

        Ok(())
    }

    /// Call to read the DP CTRL/STAT register.
    ///
    /// Returns:
    /// - `Ok(CtrlStat)`: if the DP CTRL/STAT register was read successfully,
    ///   where `CtrlStat` is the value read from the register.
    /// - `Err(SwdError)`: if there was an error reading the DP CTRL/STAT
    ///   register.
    pub async fn read_ctrl_stat(&mut self) -> Result<CtrlStat, SwdError> {
        let _version = self.check_version();

        // Read the DP CTRL/STAT register
        let op = SwdOp::DpRead(CtrlStatRegister::ADDRESS);
        let raw_data = self.do_read_op(op, true).await?;

        // Convert the raw data to a CtrlStat value
        Ok(CtrlStat::from(raw_data))
    }

    /// Call to check for errors in the Debug Port status.
    ///
    /// This function reads the DP CTRL/STAT register and checks for errors.
    ///
    /// Arguments:
    /// - `check_read_ok`: If true, checks that the read OK bit is set, in#
    ///   addition to the other error checks.
    ///
    /// Returns:
    /// - `Ok(())` if no errors are detected.
    /// - `Err(SwdError::DpError)` if any errors are detected, or if the read
    pub async fn check_dp_errors(&mut self, check_read_ok: bool) -> Result<(), SwdError> {
        let _version = self.check_version();

        let status: CtrlStat = self.read_ctrl_stat().await?;
        if status.has_errors() {
            warn!("DP status errors detected: {}", status.error_states());
        } else if check_read_ok && !status.readok() {
            warn!("DP read OK bit not set");
        }

        if status.has_errors() || (check_read_ok && !status.readok()) {
            return Err(SwdError::DpError);
        }

        Ok(())
    }

    /// Call to clear any errors on the Debug Port.
    ///
    /// This function writes to the ABORT register to clear any error states
    /// in the Debug Port, such as STKERR, STKCMP, WDERR, and ORUNERR.
    ///
    /// Returns:
    /// - `Ok(())` if the errors were cleared successfully.
    /// - `Err(SwdError)` if there was an error writing to the ABORT register,
    ///   or if the errors could not be cleared.
    pub async fn clear_errors(&mut self) -> Result<(), SwdError> {
        let _version = self.check_version();

        trace!("Exec:  Clear errors");
        // Clear any errors by writing to ABORT
        self.set_abort(true, true, true, true).await?;

        Timer::after(Duration::from_millis(1)).await;

        // Read the CtrlStat register to check they are now clear
        self.check_dp_errors(false).await?;

        trace!("OK:    Clear errors");
        Ok(())
    }

    /// Reads the DRW register from the Access Port multiple times in
    /// succession.  Takes care of reading from DRW directly or RDBUFF as
    /// appropriate.
    ///
    /// It only normally makes sense to call this function if the CSW AddrInc
    /// bits are set to 0b01 (auto-increment single enabled).  This is
    /// currently how CSW is initialized and can be changed with
    /// [`SwdInterface::set_addr_inc()`].
    ///
    /// Arguments:
    /// - `buf`: A mutable slice to store the read values.  The length of this
    ///   slice is the number of bytes to read.
    /// - `fast`: If true, avoids checking DP CTRL/STAT for errors until the
    ///   end of the read operation.  If this fails, the read values may be
    ///   tainted.  If false, checks for errors after each read, and only
    ///   returns data from non-errored reads.
    ///
    /// Returns:
    /// - `Ok(count)` if the register was read successfully the specified
    ///   number
    /// - `Err((SwdError, usize))` if there was an error reading the register,
    ///   where the `usize` is the number of successful reads before the error,
    ///   so the number of valid register values in buf.
    async fn read_drw_bulk(
        &mut self,
        buf: &mut [u32],
        fast: bool,
    ) -> Result<(), (SwdError, usize)> {
        let _version = self.check_version();

        let count = buf.len();
        let fast_str = if fast { "fast" } else { "slow" };
        trace!("Exec:  Read DRW Bulk {count} {fast_str}");

        // Check whether this register address requires a DP SELECT update.
        // RDBUFF never needs DP SELECT updating.
        let drw_op = SwdOp::ApRead(DrwRegister::ADDRESS);
        self.check_and_update_dp_select(drw_op)
            .await
            .map_err(|e| (e, 0))?;

        // Read the first (to be dicarded) value from the DRW register
        let _ = self.read_drw_fast().await.map_err(|e| (e, 0))?;

        // Now read the DRW AP register the appropriate number of times
        let mut read_count = 0;
        for item in buf.iter_mut().take(count - 1) {
            // Read the register
            let data = self.read_drw_fast().await.map_err(|e| (e, read_count))?;

            // Check for errors
            if !fast {
                self.check_dp_errors(true)
                    .await
                    .map_err(|e| (e, read_count))?;
            }

            // Convert the raw data to the appropriate register value type
            *item = data.into();

            // Increment the read count
            read_count += 1;
        }

        // Read the final value from the RDBUFF register
        let data = self
            .read_rd_buff_fast(true)
            .await
            .map_err(|e| (e, read_count))?;

        // Check for errors in the slow case
        if !fast {
            self.check_dp_errors(true)
                .await
                .map_err(|e| (e, read_count))?;
        }

        // Store the final value in the buffer
        buf[read_count] = data.into();
        read_count += 1;

        // Error check here in fast case
        if fast {
            self.check_dp_errors(true)
                .await
                .map_err(|e| (e, read_count))?;
        }

        Ok(())
    }

    /// Writes to the DRW register from the Access Port multiple times in
    /// succession.
    ///
    /// It only normally makes sense to call this function if the CSW AddrInc
    /// bits are set to 0b01 (auto-increment single enabled).  This is
    /// currently how CSW is initialized and can be changed with
    /// [`SwdInterface::set_addr_inc()`].
    ///
    /// However, this could validly be used to write multiple values to, say,
    /// the same hardware register, like a GPIO BSRR register - although the
    /// application wouldn't have any control over the period.
    ///
    /// Arguments:
    /// - `buf`: A slice containing the values to write.
    /// - `fast`: If true, avoids checking DP CTRL/STAT for errors until the
    ///   end of the write operation.  If false, checks for errors after each
    ///   write, and stops on the first error.
    ///
    /// Returns:
    /// - `Ok(())` if all values were written successfully
    /// - `Err((SwdError, usize))` if there was an error writing, where the
    ///   `usize` is the number of successful writes before the error.
    async fn write_drw_bulk(&mut self, buf: &[u32], fast: bool) -> Result<(), (SwdError, usize)> {
        let _version = self.check_version();

        let count = buf.len();
        let fast_str = if fast { "fast" } else { "slow" };
        trace!("Exec:  Write DRW Bulk {count} {fast_str}");

        if buf.is_empty() {
            return Ok(());
        }

        // Check whether this register address requires a DP SELECT update.
        let drw_op = SwdOp::ApWrite(DrwRegister::ADDRESS);
        self.check_and_update_dp_select(drw_op)
            .await
            .map_err(|e| (e, 0))?;

        // Write each value to the DRW register
        let mut write_count = 0;
        for &value in buf {
            // Is this the final value?
            let last = write_count == (count - 1);

            // Write the register
            self.write_drw_fast(value.into(), last)
                .await
                .map_err(|e| (e, write_count))?;

            // Check for errors
            self.check_dp_errors(false)
                .await
                .map_err(|e| (e, write_count))?;

            write_count += 1;
        }

        // Error check here in fast case.  Can't combine with last one above,
        // as it needs to happen after we increment write_count
        if fast {
            self.check_dp_errors(false)
                .await
                .map_err(|e| (e, write_count))?;
        }

        Ok(())
    }

    /// Read a Debug Port register by raw address.  Use with caution.
    ///
    /// This function automatically handles setting the DP SELECT register if
    /// it is required.
    ///
    /// Arguments:
    /// - `register`: The raw register address (0x0, 0x4, 0x8, 0xC)
    ///
    /// Returns:
    /// - `Ok(u32)` if the register was read successfully
    /// - `Err(SwdError)` if there was an error reading the register.
    pub async fn read_dp_register_raw(&mut self, register: u8) -> Result<u32, SwdError> {
        let _version = self.check_version();

        self.check_power = false;
        let op = SwdOp::DpRead(register);
        let result = self.do_read_op(op, true).await;
        self.check_power = true;
        result
    }

    /// Write a Debug Port register by raw address.  Use with caution.
    ///
    /// This function automatically handles setting the DP SELECT register if
    /// it is required.
    ///
    /// Arguments:
    /// - `register`: The raw register address (0x0, 0x4, 0x8, 0xC)
    /// - `value`: The raw 32-bit value to write
    ///
    /// Returns:
    /// - `Ok(())` if the register was written successfully.
    /// - `Err(SwdError)` if there was an error writing the register.
    pub async fn write_dp_register_raw(
        &mut self,
        register: u8,
        value: u32,
    ) -> Result<(), SwdError> {
        let _version = self.check_version();

        self.check_power = false;
        let op = SwdOp::DpWrite(register);
        self.do_write_op(op, value, true)
            .await
            .inspect_err(|_| self.check_power = true)?;

        // Must update the stored DP SELECT register if we wrote it
        if register == SelectRegister::ADDRESS {
            // If we wrote the DP SELECT register, update the internal state
            self.dp_select = Select::from(value);
        }

        self.check_power = true;

        Ok(())
    }

    /// Read an Access Port register by raw address and AP index.  Use with
    /// caution.
    ///
    /// This function automatically handles setting the DP SELECT register for both
    /// AP selection and register bank selection.
    ///
    /// Arguments:
    /// - `ap_index`: The AP index (0-255)
    /// - `register`: The raw register address (0x0, 0x4, 0x8, 0xC)
    ///
    /// Returns:
    /// - `Ok(u32)` if the register was read successfully
    /// - `Err(SwdError)` if there was an error reading the register.
    pub async fn read_ap_register_raw(
        &mut self,
        ap_index: u8,
        register: u8,
    ) -> Result<u32, SwdError> {
        let _version = self.check_version();

        self.check_power = false;
        // Update DP SELECT register to select the correct AP and register bank
        let mut select = self.dp_select;
        select.set_apsel(ap_index as u32);
        select.set_apbanksel_from_addr(register);

        if select != self.dp_select {
            trace!("Exec:  Update DP SELECT {select}");
            self.update_dp_select(select)
                .await
                .inspect_err(|_| self.check_power = true)?;
        } else {
            trace!("Value: DP SELECT unchanged {select}");
        }

        // Do the AP read
        let op = SwdOp::ApRead(register);
        self.do_read_op(op, false)
            .await
            .inspect_err(|_| self.check_power = true)?;

        // Now the DP read to get the data
        let op = SwdOp::DpRead(RdBuffRegister::ADDRESS);
        self.do_read_op(op, true)
            .await
            .inspect_err(|_| self.check_power = true)
    }

    /// Write an Access Port register by raw address and AP index.  Use with
    /// caution.
    ///
    /// This function automatically handles setting the DP SELECT register for both
    /// AP selection and register bank selection.
    ///
    /// Important - you should call [`Self::set_addr_inc()`] to set the
    /// address increment mode if desired before calling this function.
    ///
    /// Arguments:
    /// - `ap_index`: The AP index (0-255)
    /// - `register`: The raw register address (0x0, 0x4, 0x8, 0xC)
    /// - `value`: The raw 32-bit value to write
    ///
    /// Returns:
    /// - `Ok(())` if the register was written successfully.
    /// - `Err(SwdError)` if there was an error writing the register.
    pub async fn write_ap_register_raw(
        &mut self,
        ap_index: u8,
        register: u8,
        value: u32,
    ) -> Result<(), SwdError> {
        let _version = self.check_version();

        self.check_power = false;
        // Update DP SELECT register to select the correct AP and register bank
        let mut select = self.dp_select;
        select.set_apsel(ap_index as u32);
        select.set_apbanksel_from_addr(register);

        if select != self.dp_select {
            self.update_dp_select(select)
                .await
                .inspect_err(|_| self.check_power = true)?;
        }

        let op = SwdOp::ApWrite(register);
        self.do_write_op(op, value, true)
            .await
            .inspect_err(|_| self.check_power = true)
    }

    /// Read an Access Port register by raw address and AP index, reading the
    /// RDBUFF register multiple times in succession.
    ///
    /// This function automatically handles setting the DP SELECT register for
    /// both AP selection and register bank selection.
    ///
    /// Important - you should call [`Self::set_addr_inc()`] to set the
    /// address increment mode if desired before calling this function.
    ///
    /// Be careful of SWD memory wrapping (usually at 1KB boundaries) if using
    /// this function directly.
    ///
    /// Arguments:
    /// - `ap_index`: The AP index (0-255)
    /// - `register`: The raw register address (0x0, 0x4
    /// - `buf`: A mutable slice to store the read values.  The length of this
    ///   slice is the number of bytes to read.
    ///
    /// Returns:
    /// - `Ok(())` if the register was read successfully the specified
    ///   number of times.
    /// - `Err((SwdError, usize))` if there was an error reading the register,
    ///   where the `usize` is the number of successful reads before the error,
    ///   so the number of valid register values in buf.
    pub async fn read_ap_register_raw_bulk(
        &mut self,
        ap_index: u8,
        register: u8,
        buf: &mut [u32],
    ) -> Result<(), (SwdError, usize)> {
        let _version = self.check_version();

        self.check_power = false;

        // Update DP SELECT register to select the correct AP and register bank
        let mut select = self.dp_select;
        select.set_apsel(ap_index as u32);
        select.set_apbanksel_from_addr(register);

        if select != self.dp_select {
            self.update_dp_select(select)
                .await
                .inspect_err(|_| self.check_power = true)
                .map_err(|e| (e, 0))?;
        }

        // Read the RDBUFF register multiple times
        self.read_drw_bulk(buf, true)
            .await
            .inspect_err(|_| self.check_power = true)
    }

    /// Write an Access Port register by raw address and AP index, writing the
    /// RDBUFF register multiple times in succession.
    ///
    /// This function automatically handles setting the DP SELECT register for
    /// both AP selection and register bank selection.
    ///
    /// Important - you should call [`Self::set_addr_inc()`] to set the
    /// address increment mode if desired before calling this function.
    ///
    /// Be careful of SWD memory wrapping (usually at 1KB boundaries) if using
    /// this function directly.
    ///
    /// Arguments:
    /// - `ap_index`: The AP index (0-255)
    /// - `register`: The raw register address (0x0, 0x4
    /// - `buf`: A mutable slice to store the write values.  The length of this
    ///   slice is the number of bytes to write.
    ///
    /// Returns:
    /// - `Ok(())` if the register was written successfully the specified
    ///   number of times.
    /// - `Err((SwdError, usize))` if there was an error writing the register,
    ///   where the `usize` is the number of successful writes before the error,
    ///   so the number of valid register values in buf.
    pub async fn write_ap_register_raw_bulk(
        &mut self,
        ap_index: u8,
        register: u8,
        buf: &[u32],
    ) -> Result<(), (SwdError, usize)> {
        let _version = self.check_version();

        self.check_power = false;

        // Update DP SELECT register to select the correct AP and register bank
        let mut select = self.dp_select;
        select.set_apsel(ap_index as u32);
        select.set_apbanksel_from_addr(register);

        if select != self.dp_select {
            self.update_dp_select(select)
                .await
                .inspect_err(|_| self.check_power = true)
                .map_err(|e| (e, 0))?;
        }

        // Read the RDBUFF register multiple times
        self.write_drw_bulk(buf, true)
            .await
            .inspect_err(|_| self.check_power = true)
    }

    /// Clocks the SWD interface to the specified level, for the specified
    /// number of cycles.
    ///
    /// This is a low-level operation that directly controls the SWD clock
    /// line (SWCLK).  It can be used to clock individual bits onto the SWD
    /// interface if required, or add additional (low) clock cycles.
    ///
    /// It would be possible to implement a custom JTAG to SWD reset sequence
    /// using this function, but it would be substantially less efficient than
    /// the standard [`Self::reset_sequence_v1()`] function.
    ///
    /// Arguments:
    /// - `level`: The level to set the SWDIO line to before clocking.
    /// - `post_level`: The level to set the SWDIO line to after clocking.
    /// - `cycles`: The number of clock cycles to perform.
    pub fn clock_raw(&mut self, level: LineState, post_level: LineState, cycles: u32) {
        let _version = self.check_version();

        trace!("Exec:  Clock {level:?} {cycles} {post_level:?}");
        // Set the SWDIO line to the desired state
        level.set_swdio_state(&mut self.protocol);

        // Clock the SWD interface for the specified number of cycles
        self.protocol.clock(cycles);

        // Set the SWDIO line to the post-level state
        post_level.set_swdio_state(&mut self.protocol);
    }

    /// Powers up the debug domain of the target device.
    ///
    /// Once the initialization sequence has been completed, call this method
    /// to power up the target's debug domain.  This allows further operations,
    /// such as `configure_mem_ap()` to be performed.
    ///
    /// Returns:
    /// - `Ok(())`: if the debug domain was successfully powered up.
    /// - `Err(SwdError)`: if there was an error powering up the debug domain,
    ///   such as if the target is not responding, or if the power-up request
    ///   failed.
    pub async fn power_up_debug_domain(&mut self) -> Result<(), SwdError> {
        let _version = self.check_version()?;

        // Set default DP SELECT
        self.update_dp_select(Select::default()).await?;

        // Power up debug domain
        let mut ctrl_stat = CtrlStat::default();
        ctrl_stat.set_cdbgpwrupreq(true);
        ctrl_stat.set_csyspwrupreq(true);
        self.write_dp_register(CtrlStatRegister, ctrl_stat).await?;

        // Verify power up
        let status = self.read_dp_register(CtrlStatRegister).await?;
        if !status.cdbgpwrupack() || !status.csyspwrupack() {
            return Err(SwdError::OperationFailed(
                "debug domain power up failed".to_string(),
            ));
        }

        debug!("OK:   Debug domain powered up {}", status.power_states());
        self.powered_up = true;

        Ok(())
    }

    /// Configures the MEM-AP for access.
    ///
    /// This should be called before calling functions which attempt to read
    /// or write to the target's address space.  This includes 'get_mcu()`,
    /// `read_mem()`, and `write_mem()`.
    ///
    /// Returns:
    /// - `Ok(())`: if the MEM-AP was successfully configured.
    /// - `Err(SwdError)`: if there was an error configuring the MEM-AP, such
    ///   as if the CSW register could not be read or written, or if the
    ///   configuration did not match the expected values after writing.
    pub async fn configure_mem_ap(&mut self) -> Result<(), SwdError> {
        if self.check_power && !self.powered_up {
            return Err(SwdError::NotReady);
        }

        // Read CSW register (unused)
        let _ = self.read_ap_register(CswRegister).await?;

        // Configure MEM_AP
        let mut new_csw = Csw::default();
        let addr_inc = if self.addr_inc {
            Csw::ADDRINC_SINGLE
        } else {
            Csw::ADDRINC_OFF
        };
        new_csw.set_addrinc(addr_inc);
        self.write_ap_register(CswRegister, new_csw).await?;

        // Double check CSW register
        let csw_readback: Csw = self.read_ap_register(CswRegister).await?;

        // Update our addr_inc field based on the readback
        self.addr_inc = csw_readback.addrinc() != Csw::ADDRINC_OFF;

        // Compare readback with what we set.  However, bits 24-30 tend to vary
        // based on MCU, so accept any values in those bits.
        trace!("Value: CSW readback {csw_readback}");
        let csw_readback_check = csw_readback.value() & 0xFFFFFF;
        let set_csw = new_csw.value() & 0xFFFFFF;
        if csw_readback_check != set_csw {
            warn!("CSW configuration mismatch after write: expected {new_csw}, got {csw_readback}");
            //return Err(SwdError::OperationFailed(
            //    "csw configuration mismatch".to_string(),
            //));
        }

        // Read the IDR and check it's a MEM-AP
        let idr: Idr = self.read_ap_register(IdrRegister).await?;
        self.idr = Some(idr);
        for check_idr in IDR_AHB_AP_KNOWN {
            if idr == check_idr {
                trace!("Value: MEM-AP IDR {idr} matches known IDR {check_idr}");
                return Ok(());
            }
        }

        // We do not error if we don't recognise a known MEM-AP, instead we
        // log
        warn!("Unknown MEM-AP IDR {idr}");

        Ok(())
    }

    /// Retrieves the IDCODE of the target device, if available.
    pub fn idcode(&self) -> Option<IdCode> {
        self.idcode
    }

    /// Retrieves the MCU information, if available.
    pub fn mcu(&self) -> Option<Mcu> {
        self.mcu
    }

    /// Retrieves the IDR of the MEM-AP, if available.
    pub fn idr(&self) -> Option<Idr> {
        self.idr
    }

    /// Retrieves whether the CSW AddrInc is set to auto-increment single
    /// (0b01).
    pub fn addr_inc(&self) -> bool {
        self.addr_inc
    }

    /// Sets the number of automatic retries after each SWD operation is a
    /// WAIT ack is received.
    pub fn set_wait_retries(&mut self, retries: u32) {
        self.wait_retries = retries;
    }

    /// Sets the CSW AddrInc field to the given value.
    ///
    /// This function reads the CSW register, checks the current AddrInc
    /// value, and sets it to the given value if it is not already set to that
    /// value.  It then writes the CSW register back to the target and checks
    /// that the value was set correctly.
    ///
    /// Arguments:
    /// - `addr_inc`: If true, sets the AddrInc to SINGLE (0b01).
    ///   If false, sets the AddrInc to OFF (0b00).
    ///
    /// Returns:
    /// - `Ok(())`: if the AddrInc was successfully set.
    /// - `Err(SwdError)`: if there was an error reading or writing the CSW
    ///   register, or if the AddrInc value was not set correctly after
    ///   writing.
    ///
    pub async fn set_addr_inc(&mut self, addr_inc: bool) -> Result<(), SwdError> {
        // Read CSW
        let mut csw: Csw = self.read_ap_register(CswRegister).await?;

        let cur_addr_inc = csw.addrinc();
        #[allow(clippy::if_same_then_else)]
        if cur_addr_inc == Csw::ADDRINC_OFF && !addr_inc {
            // No change needed
            return Ok(());
        } else if cur_addr_inc == Csw::ADDRINC_SINGLE && addr_inc {
            // No change needed
            return Ok(());
        }

        // Set the new AddrInc value
        let new_addr_inc = if addr_inc {
            Csw::ADDRINC_SINGLE
        } else {
            Csw::ADDRINC_OFF
        };
        csw.set_addrinc(new_addr_inc);

        // Write CSW
        self.write_ap_register(CswRegister, csw).await?;

        // Read back CSW to check it was set correctly
        let final_csw: Csw = self.read_ap_register(CswRegister).await?;
        if final_csw != csw {
            warn!("CSW AddrInc write failed: expected {csw}, got {final_csw}");
            return Err(SwdError::OperationFailed(
                "csw addrinc write failed".to_string(),
            ));
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
        // Set address to read data from
        let tar = Tar::from(addr);
        self.write_ap_register(TarRegister, tar).await?;

        let tar_readback: Tar = self.read_ap_register(TarRegister).await?;
        if tar != tar_readback {
            warn!("TAR readback mismatch: expected {tar}, got {tar_readback}");
            return Err(SwdError::OperationFailed(format!(
                "unexpect tar {tar_readback}"
            )));
        }

        // Read the data from the address in the TAR register
        let data = self.read_ap_register(DrwRegister).await?;

        Ok(data.into())
    }

    /// Writes a 32-bit value to the target's memory at the specified address.
    ///
    /// This address can usually be RAM, flash, or any other memory-mapped
    /// location in the target's address space, such as peripheral registers.
    ///
    /// Note that to write to flash, the MCU usually requires magic values be
    /// written to its flash register(s) before it can be programmed.  See
    /// [`DebugInterface::unlock_flash()`].
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
        // Set address to write data to
        let tar = Tar::from(addr);
        self.write_ap_register(TarRegister, tar).await?;

        let tar_readback: Tar = self.read_ap_register(TarRegister).await?;
        if tar != tar_readback {
            warn!("TAR readback mismatch: expected {tar}, got {tar_readback}");
            return Err(SwdError::OperationFailed(format!(
                "unexpected tar {tar_readback}"
            )));
        }

        // Write the data to the address in the TAR register
        let data = data.into();
        self.write_ap_register(DrwRegister, data).await?;

        Ok(())
    }

    /// Reads a block of memory from the target device.
    ///
    /// Is aware of SWD memory wrapping and handles it (at the 1KB boundary)
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
        if addr & 0x3 != 0 {
            info!("Error: Attempt to read on non-4 byte boundary");
            return Err((SwdError::Api, 0));
        }

        let mut remaining = buf;
        let mut current_addr = addr;
        let mut total_read = 0;

        while !remaining.is_empty() {
            // Calculate words before 1KB boundary
            let boundary_offset = SWD_MEMORY_BOUNDARY - (current_addr & (SWD_MEMORY_BOUNDARY - 1));
            let max_words = (boundary_offset / 4) as usize;
            let chunk_size = remaining.len().min(max_words);

            // Set TAR for this chunk
            let tar = Tar::from(current_addr);
            self.write_ap_register(TarRegister, tar)
                .await
                .map_err(|e| (e, total_read))?;

            // Read this chunk
            let (chunk, rest) = remaining.split_at_mut(chunk_size);
            self.read_drw_bulk(chunk, fast)
                .await
                .map_err(|(e, partial)| (e, total_read + partial))?;

            // Update for next iteration
            remaining = rest;
            current_addr += (chunk_size * 4) as u32;
            total_read += chunk_size;
        }

        Ok(())
    }

    /// Writes a block of memory to the target device.
    ///
    /// Is aware of SWD memory wrapping and handles it (at the 1KB boundary)
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
        if addr & 0x3 != 0 {
            info!("Error: Attempt to write on non-4 byte boundary");
            return Err((SwdError::Api, 0));
        }

        let mut remaining = buf;
        let mut current_addr = addr;
        let mut total_written = 0;

        while !remaining.is_empty() {
            // Calculate words before 1KB boundary
            let boundary_offset = SWD_MEMORY_BOUNDARY - (current_addr & (SWD_MEMORY_BOUNDARY - 1));
            let max_words = (boundary_offset / 4) as usize;
            let chunk_size = remaining.len().min(max_words);

            // Set TAR for this chunk
            let tar = Tar::from(current_addr);
            self.write_ap_register(TarRegister, tar)
                .await
                .map_err(|e| (e, total_written))?;

            // Write this chunk
            let (chunk, rest) = remaining.split_at(chunk_size);
            self.write_drw_bulk(chunk, fast)
                .await
                .map_err(|(e, partial)| (e, total_written + partial))?;

            // Update for next iteration
            remaining = rest;
            current_addr += (chunk_size * 4) as u32;
            total_written += chunk_size;
        }

        Ok(())
    }
}

// Internal functions
impl<'a> SwdInterface<'a> {
    async fn do_write_op(&mut self, op: SwdOp, data: u32, single: bool) -> Result<(), SwdError> {
        let cmd = op.to_cmd();
        trace!("Exec:  {op} SWD: {cmd:#04X} {data:#010X}");

        self.protocol.set_swdio_output();

        let mut attempt = 0;
        let result = loop {
            self.protocol.write_cmd_turnaround(cmd);

            match self.protocol.read_ack() {
                Ok(()) => {
                    self.protocol.turnaround_write_u32_parity(data);

                    // STM32F4 reference manuals say 2 extra SWCLK cycles are required
                    // after a write, after the parity bit, so do that now
                    self.protocol.set_swdio_low();
                    self.protocol.clock(2);

                    // If we won't be sending another operation soon, we must clock the
                    // rest of the required number of post operation cycles.
                    if single {
                        self.protocol.clock(POST_SINGLE_OPERATION_CYCLES - 2);
                    }

                    break Ok(());
                }
                Err(SwdError::WaitAck) => trace!("Exec:  {op} Wait ACK"), // Retry
                Err(e) => break Err(e),                                   // Other errors, stop
            }

            attempt += 1;
            if attempt > self.wait_retries {
                break Err(SwdError::WaitAck);
            } else {
                trace!("Retry: {op} {}", attempt - 1);
            }
        };

        // Log result
        match &result {
            Ok(()) => trace!("OK:    {op}"),
            Err(e) => debug!("Error: {op} {data:#010X}: {e:?}"),
        }

        result
    }

    async fn do_write_target_sel(&mut self, ts: TargetSel) -> Result<(), SwdError> {
        let op = SwdOp::DpWrite(TargetSelRegister::ADDRESS);

        let cmd = op.to_cmd();
        trace!("Exec:  {op} SWD: {cmd:#04X} {ts}");

        self.protocol.set_swdio_output();

        self.protocol.write_cmd_5_undriven(cmd);

        self.protocol.write_u32_parity(ts.data());

        self.protocol.set_swdio_low();

        self.protocol.clock(2);

        trace!("OK:    {op}");

        Ok(())
    }

    // Lowest level read operation which actually drives the SWD protocol.
    async fn do_read_op(&mut self, op: SwdOp, single: bool) -> Result<u32, SwdError> {
        let cmd = op.to_cmd();
        trace!("Exec:  {op}  SWD: {cmd:#04X}");

        self.protocol.set_swdio_output();

        let mut attempt = 0;
        let result = loop {
            self.protocol.write_cmd_turnaround(cmd);

            match self.protocol.read_ack() {
                Ok(()) => {
                    // Read data + parity + turnaround bit (which leaves swdio as low)
                    let data = match self.protocol.read_u32_parity_turnaround() {
                        Ok(data) => data,
                        Err(e) => break Err(e),
                    };

                    if single {
                        // Post operation clock is 8 cycles
                        self.protocol.clock(POST_SINGLE_OPERATION_CYCLES);
                    }

                    break Ok(data);
                }
                Err(SwdError::WaitAck) => trace!("Wait:  {op}"), // Retry
                Err(e) => break Err(e),                          // Other errors, stop
            }

            attempt += 1;
            if attempt > self.wait_retries {
                break Err(SwdError::WaitAck);
            } else {
                trace!("Retry: {op} {}", attempt - 1);
            }
        };

        // Log result
        match &result {
            Ok(data) => trace!("OK:    {op}            {data:#010X}"),
            Err(e) => debug!("Error: {op}  {e:?}"),
        }

        result
    }

    async fn write_operation(
        &mut self,
        op: SwdOp,
        data: u32,
        single: bool,
    ) -> Result<(), SwdError> {
        if self.check_power && op.requires_power_up() && !self.powered_up {
            return Err(SwdError::NotReady);
        }

        self.check_and_update_dp_select(op).await?;

        self.do_write_op(op, data, single).await?;

        self.check_dp_errors(false).await
    }

    // Handles both DP and AP reads.  AP reads take 2 transactions, hence the
    // loop to handle the switch from AP read to DP read.  Therefore, this
    // function is more complex than `read_operation()`.
    async fn read_operation(&mut self, op: SwdOp, single: bool) -> Result<u32, SwdError> {
        if self.check_power && op.requires_power_up() && !self.powered_up {
            return Err(SwdError::NotReady);
        }

        self.check_and_update_dp_select(op).await?;

        let _data = match op {
            SwdOp::DpRead(_) => return self.do_read_op(op, single).await,
            SwdOp::ApRead(_) => {
                // For AP reads, we need to read RDBUFF after the initial read
                // operation, so we can't just return the result of do_read_op.
                // Instead, we will handle the switch to DP read in the loop below.
                self.do_read_op(op, false).await?
            }
            _ => {
                unreachable!("Read operation should be either DpRead or ApRead");
            }
        };

        // Now we've done the AP read, check that worked.
        self.check_dp_errors(true).await?;

        // We ignore the data from the ApRead - it won't be from this
        // operation, but may be from a previous read.  Note RDBUFF never
        // requires a DP SELECT update.
        self.do_read_op(SwdOp::DpRead(RdBuffRegister::ADDRESS), single)
            .await
    }

    async fn check_and_update_dp_select(&mut self, op: SwdOp) -> Result<(), SwdError> {
        let check = match op {
            SwdOp::DpWrite(addr) => {
                // None of these DP registers require a DP SELECT update
                !matches!(
                    addr,
                    AbortRegister::ADDRESS | SelectRegister::ADDRESS | RdBuffRegister::ADDRESS
                )
            }
            SwdOp::DpRead(addr) => {
                // Strictly the SELECT register is READ RESEND when read.
                // None of these DP registers require a DP SELECT update
                !matches!(
                    addr,
                    IdCodeRegister::ADDRESS | SelectRegister::ADDRESS | RdBuffRegister::ADDRESS
                )
            }
            SwdOp::ApWrite(_) | SwdOp::ApRead(_) => true,
        };

        if !check {
            // No DP SELECT update required
            return Ok(());
        }

        // Check whether the DP SELECT register value (that we last wrote, as
        // reading is deprecated) needs updating for this operation.
        if !op.check_dp_select(self.dp_select) {
            // Get current value
            let (mut select, _) = op.dp_select();

            // Get new value
            let select_new = match op {
                SwdOp::DpRead(addr) | SwdOp::DpWrite(addr) => {
                    select.set_dpbanksel_from_addr(addr);
                    select
                }
                SwdOp::ApRead(addr) | SwdOp::ApWrite(addr) => {
                    select.set_apbanksel_from_addr(addr);
                    select
                }
            };

            // Update the DP SELECT register
            self.update_dp_select(select_new).await?;
        }

        Ok(())
    }

    async fn read_rd_buff_fast(&mut self, last: bool) -> Result<RdBuff, SwdError> {
        // Read the RDBUFF register
        let op = SwdOp::DpRead(RdBuffRegister::ADDRESS);
        let rdbuff = self.do_read_op(op, !last).await?;
        Ok(rdbuff.into())
    }

    // Assumes DP SELECT is set, and won't be a single operation
    async fn read_drw_fast(&mut self) -> Result<Drw, SwdError> {
        // Read the DRW register
        let op = SwdOp::ApRead(DrwRegister::ADDRESS);
        let drw = self.do_read_op(op, false).await?;
        Ok(drw.into())
    }

    // Assumes DP SELECT is set, and won't be a single operation
    async fn write_drw_fast(&mut self, value: Drw, last: bool) -> Result<(), SwdError> {
        // Write the DRW register
        let op = SwdOp::ApWrite(DrwRegister::ADDRESS);
        let raw_data = value.into();
        self.do_write_op(op, raw_data, !last).await?;
        Ok(())
    }

    async fn set_abort(
        &mut self,
        stkcmpclr: bool,
        stkerrclr: bool,
        wderrclr: bool,
        orunerrclr: bool,
    ) -> Result<(), SwdError> {
        // Create the ABORT register value
        let mut abort = Abort::default();
        abort.set_stkcmpclr(stkcmpclr);
        abort.set_stkerrclr(stkerrclr);
        abort.set_wderrclr(wderrclr);
        abort.set_orunerrclr(orunerrclr);

        // Write the ABORT register
        let op = SwdOp::DpWrite(AbortRegister::ADDRESS);
        self.do_write_op(op, abort.into(), true).await?;

        Ok(())
    }

    /// Gets details about target MCU.  Currently only supports STM32 devices.
    ///
    /// Arguments:
    /// - `idcode`: The IDCODE of the target device, which is used to help
    ///   identify the MCU family and device.
    ///
    /// Returns:
    /// - `Ok(Mcu)`: if the MCU was successfully identified, containing the
    ///   details of the MCU, such as its family, device ID, unique ID, and flash
    ///   size.
    /// - `Err(SwdError)`: if there was an error reading the MCU details, such
    ///   as if the target did not respond, or if the device ID could not be
    ///   read correctly.
    async fn get_mcu(&mut self, idcode: IdCode) -> Result<Mcu, SwdError> {
        match idcode {
            Cortex::IDCODE_M3 | Cortex::IDCODE_M4 => {
                // Read the device ID from the target address
                let addr = StmDeviceId::ADDRESS;
                let data = self.read_mem(addr).await?;
                let device_id = StmDeviceId::new(data);

                // Read the Unique ID from the target address
                let uid_addr = StmUniqueId::addr_from_family(device_id.family());
                let unique_id = if let Some(uid_addr) = uid_addr {
                    let mut uid = [0; 3];
                    for (ii, uid) in uid.iter_mut().enumerate() {
                        *uid = self.read_mem(uid_addr + (ii as u32 * 4)).await?;
                    }
                    Some(StmUniqueId::new(uid))
                } else {
                    None
                };

                // Read the Flash Size Register
                let flash_size_addr = StmFlashSize::addr_from_family(device_id.family());
                let flash_size = if let Some(flash_size_addr) = flash_size_addr {
                    let flash_size_raw = self.read_mem(flash_size_addr).await?;
                    let flash_size_raw = (flash_size_raw >> 16) as u16;
                    Some(StmFlashSize::new(flash_size_raw))
                } else {
                    None
                };

                let stm = StmDetails::new(device_id, idcode, unique_id, flash_size);

                let mcu = Mcu::Stm32(stm);

                Ok(mcu)
            }
            Cortex::IDCODE_M0 => {
                let chip_id = self.read_mem(rp::RP2040_CHIP_ID_ADDR).await?;
                let cpu_id = self.read_mem(rp::RP2040_CPU_ID_ADDR).await?;
                if chip_id == rp::RP2040_CHIP_ID && cpu_id == rp::RP2040_CPU_ID {
                    Ok(Mcu::Rp(RpDetails::from_line(rp::RpLine::Rp2040)))
                } else {
                    Ok(Mcu::Unknown(idcode))
                }
            }
            Cortex::IDCODE_M33 => {
                // Could be an RP2350 but don't support detecting them yet
                Ok(Mcu::Unknown(idcode))
            }
            _ => {
                info!("Info:  Unknown MCU family: {idcode}");
                Ok(Mcu::Unknown(idcode))
            }
        }
    }
}

/// SWD Operations
///
/// Each operation contains the register address as a u8 (0x0, 0x4, etc).
///
/// SWD command format
/// Bit 0: Start (1)
/// Bit 1: APnDP (0=DP, 1=AP)  
/// Bit 2: RnW (0=write, 1=read)
/// Bit 3: A2 (address bit 2)
/// Bit 4: A3 (address bit 3)  
/// Bit 5: Parity
/// Bit 6: Stop (0)
/// Bit 7: Park (1)
///
/// Use this to create low-level SWD operations directly , which can be sent
/// to the target using [`SwdInterface`] methods.
///
/// Examples:
///
/// ```rust
/// let _dp_write = SwdOp::DpRead(airfrog_core::arm::dp::CtrlStatRegister::ADDRESS);
/// let _ap_read = SwdOp::ApRead(airfrog_core::arm::map::DrwRegister::ADDRESS);
/// ```
/// Create using `SwdOp::DpRead(0x0)`, `SwdOp::ApWrite(0x4)`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwdOp {
    DpRead(u8),
    DpWrite(u8),
    ApRead(u8),
    ApWrite(u8),
}

impl SwdOp {
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_cmd(&self) -> u8 {
        // SWD cmd: [start][APnDP][RnW][A3][A2][parity][stop][park][trn]
        let (base, addr) = match self {
            // start=1, APnDP=0, RnW=1, park=1
            SwdOp::DpRead(a) => (0x85, a),
            // start=1, APnDP=0, RnW=0, park=1
            SwdOp::DpWrite(a) => (0x81, a),
            // start=1, APnDP=1, RnW=1, park=1
            SwdOp::ApRead(a) => (0x87, a),
            // start=1, APnDP=1, RnW=0, park=1
            SwdOp::ApWrite(a) => (0x83, a),
        };

        let cmd = base | ((addr & 0x0C) << 1); // A[3:2] to bits 4:3
        Self::add_parity(cmd)
    }

    fn add_parity(cmd: u8) -> u8 {
        // Parity is calculated using APnDP, RnW and A[2:3]
        // This is bits 1, 2, 3 and 4 of our implementation
        let parity_bits = cmd & 0x1E;
        let parity = calculate_parity(parity_bits) as u8;
        cmd | (parity << 5)
    }

    /// Returns the DP SELECT register value required for this operation,
    /// and the bit mask with the relevant bits.
    pub(crate) fn dp_select(&self) -> (Select, u32) {
        let mut select = Select::default();
        match self {
            SwdOp::DpRead(addr) | SwdOp::DpWrite(addr) => {
                select.set_dpbanksel_from_addr(*addr);
                (select, Select::DPBANKSEL_MASK)
            }
            SwdOp::ApRead(addr) | SwdOp::ApWrite(addr) => {
                select.set_apbanksel_from_addr(*addr);
                (select, Select::APBANKSEL_MASK)
            }
        }
    }

    /// Checks if the given SELECT register value has the correct bits already
    /// set.
    pub(crate) fn check_dp_select(&self, select: Select) -> bool {
        let (bank, mask) = match self {
            SwdOp::DpRead(addr) | SwdOp::DpWrite(addr) => {
                let bank = (((addr >> 4) & 0xF) << Select::DPBANKSEL_SHIFT) as u32;
                let mask = Select::DPBANKSEL_MASK << Select::DPBANKSEL_SHIFT;
                (bank, mask)
            }
            SwdOp::ApRead(addr) | SwdOp::ApWrite(addr) => {
                let bank = (((addr >> 4) & 0xF) << Select::APBANKSEL_SHIFT) as u32;
                let mask = Select::APBANKSEL_MASK << Select::APBANKSEL_SHIFT;
                (bank, mask)
            }
        };
        (select.value() & mask) == bank
    }

    /// Whether this operation requires the debug domain to be powered up.
    pub(crate) fn requires_power_up(&self) -> bool {
        match self {
            SwdOp::DpRead(_) | SwdOp::DpWrite(_) => false,
            SwdOp::ApRead(_) | SwdOp::ApWrite(_) => true,
        }
    }
}

impl fmt::Display for SwdOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SwdOp::DpRead(a) => write!(f, "DP Read 0x{a:02X}"),
            SwdOp::DpWrite(a) => write!(f, "DP Write 0x{a:02X}"),
            SwdOp::ApRead(a) => write!(f, "AP Read 0x{a:02X}"),
            SwdOp::ApWrite(a) => write!(f, "AP Write 0x{a:02X}"),
        }
    }
}
