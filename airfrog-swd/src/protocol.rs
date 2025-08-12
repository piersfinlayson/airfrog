// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! ARM SWD Wire Protocol Implementation
//!
//! This module implements the SWD protocol for communicating with ARM-based
//! MCUs.  It provides the `SwdProtocol` struct for low-level SWD operations.

use core::result::Result;
use embassy_time::Timer;
use esp_hal::gpio::{
    DriveMode, DriveStrength, Flex, InputConfig, InputPin, Level, Output, OutputConfig, OutputPin,
    Pull,
};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use crate::SwdError;
use airfrog_bin::Speed as BinSpeed;

#[doc(inline)]
pub use crate::debug::DebugInterface;
#[doc(inline)]
pub use crate::interface::SwdInterface;

// JTAG-to-SWD sequence as documented: 0111100111100111
const JTAG_TO_SWD_DOCUMENTED: u16 = 0b0111100111100111; // 0x79E7

// Reversed for SWD LSB-first transmission
const JTAG_TO_SWD_SEQUENCE: u16 = JTAG_TO_SWD_DOCUMENTED.reverse_bits(); // 0xE79E

const _JTAG_TO_DORMANT_SEQUENCE: u32 = 0x33BB_BBBA;

const SWD_TO_DORMANT_SEQUENCE: u16 = 0xE3BC;

const SELECTION_ALERT_SEQUENCE: [u32; 4] = [0x6209_F392, 0x8685_2D95, 0xE3DD_AFE9, 0x19BC_0EA2];

// 50+ clock cycles with SWDIO high
const LINE_RESET_SWDIO_HIGH_CYCLES: u32 = 51;

// 2+ clock cycles with SWDIO low
const LINE_RESET_SWDIO_LOW_CYCLES: u32 = 3;

// 8+ cycles with SWDIO high to begin exiting dormant mode
const DORMANT_EXIT_SWDIO_HIGH_CYCLES: u32 = 8;

// 4 cycles with SWDIO low to complete exiting dormant mode
const DORMANT_EXIT_SWDIO_LOW_CYCLES: u32 = 4;

// Defined as 0b01011000 MSB, or 0b00011010 LSB first
const SWD_ACTIVATION_CODE_SEQUENCE: u8 = 0x1a;

// Minimum 8 clocks after a single operation
pub(crate) const POST_SINGLE_OPERATION_CYCLES: u32 = 8;

/// SWD Protocol Version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Version {
    /// SWD v1.0 - Examples: STM32F1, STM32F4
    V1,

    /// SWD v2.0 - Examples: RP2040/Pico, RP2350/Pico 2
    V2,
}

/// SWD protocol speed setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Speed {
    /// Aims to be roughtly 500kHz clock
    Slow,

    /// Aims to be roughly 1MHz clock
    Medium,

    /// Aims to be roughly 2MHz clock
    Fast,

    /// Aims to be roughly 4MHz clock
    #[default]
    Turbo,
}

impl From<BinSpeed> for Speed {
    fn from(value: BinSpeed) -> Self {
        match value {
            BinSpeed::Slow => Speed::Slow,
            BinSpeed::Medium => Speed::Medium,
            BinSpeed::Fast => Speed::Fast,
            BinSpeed::Turbo => Speed::Turbo,
        }
    }
}

impl Speed {
    /// Returns the **approximate** speed in kHz for this SWD speed setting.
    pub fn speed_khz(&self) -> u32 {
        match self {
            Speed::Slow => 500,
            Speed::Medium => 1000,
            Speed::Fast => 2000,
            Speed::Turbo => 4000,
        }
    }

    fn clock_high_cycles(&self) -> u32 {
        match self {
            Speed::Slow => 75,
            Speed::Medium => 33,
            Speed::Fast => 10,
            Speed::Turbo => 0,
        }
    }

    fn clock_low_cycles(&self) -> u32 {
        match self {
            Speed::Slow => 75,
            Speed::Medium => 33,
            Speed::Fast => 10,
            Speed::Turbo => 0,
        }
    }
}

/// Possible SWD line states
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum LineState {
    /// SWD line set to low, output
    Low,

    /// SWD line set to high, output
    High,
    /// SWD line set to input
    Input,
}

impl LineState {
    /// Sets the SWDIO line to the specified state
    pub fn set_swdio_state(&self, protocol: &mut SwdProtocol) {
        match self {
            LineState::Input => protocol.set_swdio_input(),
            LineState::Low => protocol.set_swdio_low(),
            LineState::High => protocol.set_swdio_high(),
        }
    }
}

/// SWD Protocol object
///
/// This is used by [`SwdInterface`] to communicate with the target.  It is not
/// expected to be used directly by applications, hence the only public method
/// is `new()`.
///
/// You shoulduse [`DebugInterface`] (preferred) or [`SwdInterface`] instead.
///
/// Create using `SwdProtocol::new()` passing in the peripheral pins.
///
/// ```rust
/// use airfrog_swd::SwdProtocol;
///
/// let peripherals = esp_hal::init(config);
/// let swdio_pin = peripherals.GPIO0;
/// let swclk_pin = peripherals.GPIO1;
/// let swd = SwdProtocol::new(swdio_pin, swclk_pin);
/// ```
#[derive(Debug)]

pub struct SwdProtocol<'a> {
    swclk: Output<'a>,
    swdio: Flex<'a>,
    speed: Speed,
    clock_high_cycles: u32,
    clock_low_cycles: u32,
}

impl<'a> SwdProtocol<'a> {
    /// Create a new SWD protocol instance.
    ///
    /// This initializes the SWDIO and SWCLK pins for SWD communication.
    ///
    /// Arguments:
    /// - `swdio_pin`: The pin to use for SWDIO, which must implement both
    ///   `InputPin` and `OutputPin` traits.
    /// - `swclk_pin`: The pin to use for SWCLK, which must implement the
    ///   `OutputPin` trait.
    ///
    /// Returns:
    /// - A new `SwdProtocol` instance configured for SWD communication.
    pub fn new(swdio_pin: impl InputPin + OutputPin + 'a, swclk_pin: impl OutputPin + 'a) -> Self {
        // Start SWDIO as input.  We do not set a pull - it is the target's
        // responsibility to pull SWDIO high, and it only does it after we've
        // sent the JTAG-to-SWD sequence - possibly not until we've read the
        // IDCODE.
        let mut swdio = Flex::new(swdio_pin);
        let input_config = InputConfig::default().with_pull(Pull::None);
        swdio.apply_input_config(&input_config);
        swdio.set_input_enable(true);

        // Start SWCLK as output, initially LOW
        let output_config = OutputConfig::default()
            .with_drive_strength(DriveStrength::_20mA)
            .with_drive_mode(DriveMode::PushPull);
        let swclk = Output::new(swclk_pin, Level::Low, output_config);

        debug!("SWD interface created, SWDIO input without pull, SWCLK output low");

        let speed = Speed::default();
        let clock_high_cycles = speed.clock_high_cycles();
        let clock_low_cycles = speed.clock_low_cycles();

        Self {
            swclk,
            swdio,
            speed,
            clock_high_cycles,
            clock_low_cycles,
        }
    }

    pub fn speed(&self) -> Speed {
        self.speed
    }

    pub fn set_speed(&mut self, speed: Speed) {
        self.speed = speed;
        self.clock_high_cycles = speed.clock_high_cycles();
        self.clock_low_cycles = speed.clock_low_cycles();
        debug!("SWD speed set to {speed:?}");
    }

    #[inline]
    pub(crate) fn set_swdio_output(&mut self) {
        self.swdio.set_input_enable(false);
        self.swdio.set_output_enable(true);
    }

    #[inline]
    pub(crate) fn set_swdio_input(&mut self) {
        self.swdio.set_output_enable(false);
        self.swdio.set_input_enable(true);
    }

    #[inline]
    pub(crate) fn set_swdio_high(&mut self) {
        self.swdio.set_high();
    }

    #[inline]
    pub(crate) fn set_swdio_low(&mut self) {
        self.swdio.set_low();
    }

    #[inline]
    pub(crate) fn set_swclk_high(&mut self) {
        self.swclk.set_high();
    }

    #[inline]
    pub(crate) fn set_swclk_low(&mut self) {
        self.swclk.set_low();
    }

    #[inline]
    fn write_bit(&mut self, bit: bool) {
        if bit {
            self.set_swdio_high();
        } else {
            self.set_swdio_low();
        }
        self.set_swclk_low();
        riscv::asm::delay(self.clock_low_cycles);
        self.set_swclk_high();
        riscv::asm::delay(self.clock_high_cycles);
    }

    #[inline]
    fn read_bit(&mut self) -> bool {
        self.set_swclk_low();
        riscv::asm::delay(self.clock_low_cycles);

        // We read the bit before setting SWCLK high, a the micro-controller
        // uses the clk going high to trigger the next bit.  This appears to
        // take around 30ns on an STM32F4.
        let bit = self.swdio.is_high();

        self.set_swclk_high();
        riscv::asm::delay(self.clock_high_cycles);
        bit
    }

    #[inline]
    pub(crate) fn read_u32_parity_turnaround(&mut self) -> Result<u32, SwdError> {
        let mut data = 0u32;
        for ii in 0..32 {
            if self.read_bit() {
                data |= 1 << ii;
            }
        }

        // Read parity bit
        let parity = self.read_bit();
        self.turnaround_to_output();

        // Check parity
        if calculate_parity(data) != parity {
            // We will do the turnaround, as the target won't know there's been
            // an error.  We'll also do the post operation clock, because it is
            // unlikely the application will be sending another operation
            // immediately - and the caller probably won't do it even if it is
            // supposed to because of the error.
            debug!("SWD read parity error: data=0x{data:08X}, parity={parity}");
            return Err(SwdError::ReadParity);
        }

        Ok(data)
    }

    pub(crate) fn write_cmd_turnaround(&mut self, data: u8) {
        self.write_bits(8, data as u64);

        self.set_swdio_input(); // Set SWDIO to input for turnaround
        self.clock(1); // Clock for turnaround bit, leaves swclk low
    }

    // Used when writing to the TARGETSEL register.  In this case there is no
    // ACK, but rather 5 undriven cycles.
    pub(crate) fn write_cmd_5_undriven(&mut self, data: u8) {
        self.write_bits(8, data as u64);

        self.set_swdio_input();
        self.clock(5); // Leaves swclk low
        self.set_swdio_output();
    }

    #[inline]
    fn turnaround_to_output(&mut self) {
        self.clock(1);
        self.set_swdio_output();
        self.set_swdio_low();
        self.set_swclk_low();
    }

    pub(crate) fn turnaround_write_u32_parity(&mut self, data: u32) {
        self.turnaround_to_output();

        self.write_u32_parity(data);
    }

    #[inline]
    pub(crate) fn write_u32_parity(&mut self, data: u32) {
        let data: u64 = if calculate_parity(data) {
            data as u64 | (1 << 32)
        } else {
            data as u64
        };

        self.write_bits(33, data);
    }

    /// Read the ACK response from the target.  If the ACK is an error
    /// response, this will also write a turnaround bit.
    pub(crate) fn read_ack(&mut self) -> Result<(), SwdError> {
        let mut ack = 0u8;
        for ii in 0..3 {
            if self.read_bit() {
                ack |= 1 << ii;
            }
        }
        let result = SwdError::from_ack(ack);

        // Specification says we must insert a turnaround bit after a Wait
        // or Fault response.  We do so here (and also if we get an invalid
        // ACK value).  This also leaves SWDIO low.
        match &result {
            Ok(_) => (),
            Err(SwdError::WaitAck) | Err(SwdError::FaultAck) => {
                trace!("ACK error - turnaround: {result:?}");
                self.turnaround_to_output();
            }
            Err(e) => {
                trace!("ACK error - no turnaround: {e:?}");
                self.set_swdio_low();
                self.set_swclk_low();
            }
        }

        result
    }

    #[inline]
    pub(crate) fn clock(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.set_swclk_low();
            riscv::asm::delay(self.clock_low_cycles);
            self.set_swclk_high();
            riscv::asm::delay(self.clock_high_cycles);
        }

        self.set_swclk_low(); // Leave SWCLK low
    }

    // Brief pause with all lines low so we start from a known state
    pub(crate) async fn reset_prep(&mut self) {
        self.set_swdio_output();
        self.set_swdio_low();
        self.set_swclk_low();
        Timer::after_micros(500).await;
    }

    // Perform line reset before JTAG-to-SWD sequence
    pub(crate) fn pre_line_reset(&mut self) {
        // 50+ clock cycles with SWDIO high
        self.set_swdio_high();
        self.clock(LINE_RESET_SWDIO_HIGH_CYCLES);
    }

    // Perform line reset after JTAG-to-SWD sequence.  Includes 2+ clock cycles
    // with SWDIO low.
    pub(crate) async fn line_reset_after(&mut self) {
        self.set_swdio_output();

        // 50+ clock cycles with SWDIO high
        self.set_swdio_high();
        self.clock(LINE_RESET_SWDIO_HIGH_CYCLES);

        // 2+ clock cycles with SWDIO low
        self.set_swdio_low();
        self.clock(LINE_RESET_SWDIO_LOW_CYCLES);

        // Brief pause
        Timer::after_micros(100).await;
    }

    #[inline]
    pub(crate) fn write_bits(&mut self, count: usize, data: u64) {
        trace!("Info:  Writing {count} bits: 0x{data:0X}");
        let mut data = data;
        for _ in 0..count {
            self.write_bit(data & 1 == 1);
            data >>= 1;
        }
        self.set_swclk_low(); // Leave SWCLK low
    }

    pub(crate) fn jtag_to_swd_sequence(&mut self) {
        self.write_bits(16, JTAG_TO_SWD_SEQUENCE as u64);
        self.set_swdio_high(); // Set swdio to high when we're done
        self.set_swclk_low(); // And clock to low
    }

    pub(crate) fn _jtag_to_dormant_sequence(&mut self) {
        self.write_bits(31, _JTAG_TO_DORMANT_SEQUENCE as u64);
        self.set_swdio_high(); // Set swdio to high when we're done
        self.set_swclk_low(); // And clock to low
    }

    pub(crate) fn swd_to_dormant_sequence(&mut self) {
        // Send the SWD-to-Dormant sequence
        self.write_bits(16, SWD_TO_DORMANT_SEQUENCE as u64);
        self.set_swdio_high(); // Set swdio to high when we're done
        self.set_swclk_low(); // And clock to low
    }

    pub(crate) fn pre_sel_alert_seq(&mut self) {
        self.set_swdio_output();
        self.set_swdio_high();
        self.clock(DORMANT_EXIT_SWDIO_HIGH_CYCLES);
    }

    pub(crate) fn sel_alert_seq(&mut self) {
        // Send the selection alert sequence
        for &data in SELECTION_ALERT_SEQUENCE.iter() {
            self.write_bits(32, data as u64);
        }
    }

    pub(crate) fn post_sel_alert_seq(&mut self) {
        // 4 cycles with SWDIO low to complete exiting dormant mode
        self.set_swdio_low();
        self.clock(DORMANT_EXIT_SWDIO_LOW_CYCLES);
    }

    pub(crate) fn swd_act_code(&mut self) {
        self.write_bits(8, SWD_ACTIVATION_CODE_SEQUENCE as u64);
    }
}

/// Calculate SWD parity - 1 for an odd number of bits set to 1, 0 otherwise.
pub(crate) fn calculate_parity<T>(value: T) -> bool
where
    T: Into<u64>,
{
    (value.into().count_ones() % 2) == 1
}
