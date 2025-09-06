// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Firmware handling library
//!
//! This module contains firmware handling routines for different types of
//! firmware supported by airfrog:
//! - [One ROM](https://piers.rocks/u/one)
//!
//! These routines allow airfrog to load, parse and report information about
//! the firmware on the device airfrog is attached to.
//!
//! For guidance on building your own custom firmware handling plug-in, see
//! [`types`].

extern crate alloc;

use airfrog_core::Mcu;
use airfrog_rpc::io::{Reader, Writer};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, TimeoutError, with_timeout};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use serde_json::Value;
use static_cell::make_static;

use crate::firmware::rtt::{Control as RttControl, rtt_control};
use crate::http::{Method, StatusCode};
use crate::target::{
    Command as TargetCommand, REQUEST_CHANNEL_SIZE, Request as TargetRequest,
    Response as TargetResponse,
};

// Custom firmware modules
mod onerom;
mod onerom_lab;

// Modules used elsewhere;
pub(crate) mod assets;
pub(crate) mod rtt;
pub(crate) mod types;

pub(crate) use types::{Firmware, FirmwareRegistry, FirmwareType, WwwButton};

// Channel for receiving commands
type RspCh = Option<&'static Signal<CriticalSectionRawMutex, Response>>;
type ChArg = (Command, RspCh);
static FW_CMD_CHANNEL: Channel<CriticalSectionRawMutex, ChArg, 2> = Channel::new();

// Control signal for Firmware task
static FW_CTRL_SIGNAL: Signal<CriticalSectionRawMutex, Control> = Signal::new();

/// Firmware runs a timer to ensure that async Firmware tasks do not take too
/// long.  If they take longer than this Duration they are cancelled.
pub const FIRMWARE_HANDLER_TIMEOUT: Duration = Duration::from_millis(2000);

/// Firmware task commands.  Note that start/stop are controlled via the
/// control signal, not commands.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Command {
    /// Retrieve the status of the Fw task.  Returns Response::Status
    GetStatus,

    /// Triggers a fresh detect/read of the firmware from the target.  Note it
    /// is not necessary to do this upon Start - Fw will automatically attempt
    /// firmware detection.  However, this can be useful if, for example, some
    /// firmware state may have changed.  Returns Response::Firmware
    #[allow(dead_code)]
    Refresh,

    /// Returns the detected firmware type using Response::Firmware.
    #[allow(dead_code)]
    GetFirmware,

    /// Retrieves a set of key value pairs summarising the firmware.  This is
    /// rendered by the caller.  Returns Response::Kvp
    GetSummaryKvp,

    /// Retrieves an HTML document listing describing the firmware, in detail.
    /// Returns Response::Html
    GetFullHtml,

    /// Retrieves any custom buttons for this firmware.
    GetButtons,

    /// Commands Firmware to handle a REST request for /api/firmware/path.
    /// Returns Response::Json.
    #[allow(dead_code)]
    HandleRest {
        method: Method,
        path: String,
        body: Option<Value>,
    },

    /// Commands Firmware to handle a WWW request for /firmare/path.
    /// Returns Response::Html.
    #[allow(dead_code)]
    HandleWww {
        method: Method,
        path: String,
        body: Option<String>,
    },
}

pub async fn fw_command_wait(
    command: Command,
    firmware_response_signal: &'static Signal<CriticalSectionRawMutex, Response>,
) -> Response {
    firmware_response_signal.reset();
    FW_CMD_CHANNEL
        .send((command, Some(firmware_response_signal)))
        .await;
    firmware_response_signal.wait().await
}

/// Responses from Firmware for Commands.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Response {
    Status(State),
    Firmware(FirmwareType),
    Kvp(Vec<(String, String)>),
    Buttons(Vec<WwwButton>),
    Html {
        status: StatusCode,
        body: Option<String>,
    },
    Json {
        status: StatusCode,
        body: Option<Value>,
    },
    Error(Error),
}

/// Firmware task errors
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Error {
    /// Command can't be performed as task is stopped
    Stopped,

    /// We got an error from Target
    Target,

    /// Unknown Firmware
    UnknownFirmware,

    /// Used by firmware plugins to indicate a specific method is not
    /// implemented for that firmware type.
    NotImplemented,

    /// Attempt to use a method which requires aligned access, with unaligned
    /// address or data.
    NotAligned,

    /// Custom firmware implementation error
    Custom(String),

    /// Timed out
    Timeout,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Stopped => write!(f, "Firmware task is stopped"),
            Error::Target => write!(f, "Error from Target"),
            Error::UnknownFirmware => write!(f, "Unknown firmware"),
            Error::NotImplemented => write!(f, "Method not implemented for this firmware"),
            Error::NotAligned => write!(f, "Address or data not 4-byte aligned"),
            Error::Custom(s) => write!(f, "Error processing firmware: {s}"),
            Error::Timeout => write!(f, "Operation timed out"),
        }
    }
}

/// Control signals for the Firmware Task.  Called by Target once connected,
/// with the MCU details.
#[derive(Debug)]
pub enum Control {
    Start { mcu: Mcu },
    Stop,
}

/// Sends control signal to Firmware task.
pub fn fw_control(control: Control) {
    FW_CTRL_SIGNAL.signal(control);
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum State {
    Started,
    Stopped,
}

#[embassy_executor::task]
pub async fn task(
    spawner: embassy_executor::Spawner,
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
) {
    // Spawn the RTT task.  It will be started later, when we are started,
    // successfully parse the firmware, and get an RTT control block location
    // from it.
    spawner.must_spawn(rtt::rtt_task(target_sender));

    // Create our control signal
    let signal = Signal::new();
    let signal = make_static!(signal);
    let mut fw = Fw::new(target_sender, signal);

    loop {
        // Main task loop - handle:
        // - Control signals (start/stop) from Target
        // - Commands from Http
        match select(FW_CTRL_SIGNAL.wait(), FW_CMD_CHANNEL.receive()).await {
            Either::First(control) => fw.handle_control(control).await,
            Either::Second((command, response_sender)) => {
                match with_timeout(
                    FIRMWARE_HANDLER_TIMEOUT,
                    fw.handle_command(command.clone(), response_sender),
                )
                .await
                {
                    Ok(()) => (),
                    Err(TimeoutError) => {
                        warn!("Firmware command {command:?} timed out");
                        if let Some(sender) = response_sender {
                            sender.signal(Response::Error(Error::Timeout));
                        }
                    }
                }
            }
        }
    }
}

struct Fw {
    state: State,
    mcu: Option<Mcu>,
    firmware: Option<Box<dyn Firmware<FirmwareReader, FirmwareWriter>>>,
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
    target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
}

impl Fw {
    fn new(
        target_sender: Sender<
            'static,
            CriticalSectionRawMutex,
            TargetRequest,
            REQUEST_CHANNEL_SIZE,
        >,
        target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
    ) -> Self {
        Self {
            state: State::Stopped,
            mcu: None,
            firmware: None,
            target_sender,
            target_response_signal,
        }
    }

    // Handles control signals, sent by Target.
    async fn handle_control(&mut self, control: Control) {
        match control {
            Control::Start { mcu } => self.start(mcu).await,
            Control::Stop => self.stop(),
        }
    }

    // Handles commands, sent by Http.
    async fn handle_command(
        &mut self,
        command: Command,
        response_sender: Option<&'static Signal<CriticalSectionRawMutex, Response>>,
    ) {
        debug!("Exec:  Firmware command: {command:?}");

        // Handle stopped state first, and return
        if self.is_stopped() {
            debug!("Info:  Firmware task is stopped");
            let response = if command == Command::GetStatus {
                Response::Status(self.state)
            } else {
                Response::Error(Error::Stopped)
            };

            debug!("Info:  Response: {response:?}");
            if let Some(sender) = response_sender {
                sender.signal(response);
            }

            return;
        }

        // Now handle any commands we don't _need_ to have decoded fimware for
        let response = match command {
            Command::GetStatus => Some(Response::Status(self.state)),
            Command::Refresh => {
                let fw_type = self.detect_and_decode_firmware().await;
                Some(Response::Firmware(fw_type))
            }
            Command::GetFirmware => {
                if let Some(firmware) = &self.firmware {
                    Some(Response::Firmware(firmware.fw_type()))
                } else {
                    Some(Response::Firmware(FirmwareType::Unknown))
                }
            }
            _ => None,
        };
        if let Some(response) = response {
            debug!("Info:  Response: {response:?}");
            if let Some(sender) = response_sender {
                sender.signal(response);
            }

            // Return whether we sent a response or not
            return;
        }

        // For the rest we need a firmware type, check if we have one
        if self.firmware.is_none() {
            debug!("Info:  Response: {response:?}");
            if let Some(sender) = response_sender {
                sender.signal(Response::Error(Error::UnknownFirmware));
            }

            // Return whether we sent a response or not
            return;
        }

        // Now we must have firmware, so handle the rest of the commands appropriately
        let fw = self.firmware.as_ref().unwrap();
        let response = match command {
            Command::GetStatus | Command::Refresh | Command::GetFirmware => unreachable!(), // Handled above
            Command::GetSummaryKvp => fw
                .get_summary_kvp()
                .map_or_else(Response::Error, Response::Kvp),
            Command::GetFullHtml => {
                fw.get_full_html()
                    .map_or_else(Response::Error, |(status, body)| Response::Html {
                        status,
                        body,
                    })
            }
            Command::GetButtons => fw
                .get_buttons()
                .map_or_else(Response::Error, Response::Buttons),
            Command::HandleRest { method, path, body } => {
                let mut reader = self.reader();
                let mut writer = self.writer();
                fw.handle_rest(method, path, body, &mut reader, &mut writer)
                    .await
                    .map_or_else(Response::Error, |(status, body)| Response::Json {
                        status,
                        body,
                    })
            }
            Command::HandleWww { method, path, body } => {
                let mut reader = self.reader();
                let mut writer = self.writer();
                fw.handle_www(method, path, body, &mut reader, &mut writer)
                    .await
                    .map_or_else(Response::Error, |(status, body)| Response::Html {
                        status,
                        body,
                    })
            }
        };

        debug!("Info:  Response: {response:?}");

        if let Some(sender) = response_sender {
            sender.signal(response);
        }
    }

    // Create a new reader instance dynamically, ensures no borrow checker
    // conflicts
    fn reader(&self) -> FirmwareReader {
        FirmwareReader::new(self.target_sender, self.target_response_signal)
    }

    // Create a new writer instance dynamically, ensures no borrow checker
    fn writer(&self) -> FirmwareWriter {
        FirmwareWriter::new(self.target_sender, self.target_response_signal)
    }

    async fn detect_and_decode_firmware(&mut self) -> FirmwareType {
        assert!(self.mcu.is_some());
        let mcu = self.mcu.as_ref().unwrap();
        let mut reader = self.reader();
        if let Some(fw) = FirmwareRegistry::detect_and_decode(mcu, &mut reader).await {
            let fw_type = fw.fw_type();
            self.firmware = Some(fw);
            fw_type
        } else {
            self.firmware = None;
            FirmwareType::Unknown
        }
    }

    fn stop(&mut self) {
        if self.state != State::Stopped {
            info!("Exec:  Firmware task stopped");
            self.state = State::Stopped;
        } else {
            trace!("Info:  Firmware task already stopped");
        }
        self.mcu = None;
        self.firmware = None;

        rtt_control(RttControl::Stop);
    }

    async fn start(&mut self, mcu: Mcu) {
        info!("Exec:  Firmware task started for MCU: {mcu}");
        self.mcu = Some(mcu);
        self.state = State::Started;

        // Get the firmware
        self.detect_and_decode_firmware().await;

        // If we have firmware, and have an RTT control block address, start
        // RTT.
        if let Some(fw) = self.firmware.as_ref()
            && let Some(rtt_cb_loc) = fw.rtt_cb_address()
        {
            rtt_control(RttControl::Start { rtt_cb_loc });
            debug!("RTT control started at {rtt_cb_loc:#010X}");
        }
    }

    fn is_stopped(&self) -> bool {
        self.state == State::Stopped
    }

    fn _is_started(&self) -> bool {
        self.state == State::Started
    }
}

/// Reader instance used by custom Firmware implementations to read firmware
/// data from the Target.
pub struct FirmwareReader {
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
    target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
}

impl FirmwareReader {
    fn new(
        target_sender: Sender<
            'static,
            CriticalSectionRawMutex,
            TargetRequest,
            REQUEST_CHANNEL_SIZE,
        >,
        target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
    ) -> Self {
        Self {
            target_sender,
            target_response_signal,
        }
    }
}

impl Reader for FirmwareReader {
    type Error = Error;

    async fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        trace!(
            "Info:  FirmwareReader reading {:#010X} length {}",
            addr,
            buf.len()
        );

        let start_aligned = addr & !3;
        let end_aligned = (addr + buf.len() as u32 + 3) & !3;
        let count = ((end_aligned - start_aligned) / 4) as usize;

        // Build a Target request
        let addr_str = format!("{start_aligned:#010X}");
        let command = TargetCommand::ReadMemBulk {
            addr: addr_str,
            count,
        };
        let request = TargetRequest {
            command,
            response_signal: self.target_response_signal,
        };

        trace!("Info:  FirmwareReader send command: {:?}", request.command);

        self.target_sender.send(request).await;

        // Wait for the response
        let response = self.target_response_signal.wait().await;

        trace!("Info:  FirmwareReader got response: {response:?}");

        // Handle errors
        if let Some(error) = response.error {
            warn!("Failed to read memory from target: {error}");
            return Err(Error::Target);
        }
        if response.data.is_none() {
            warn!("No data received from target");
            return Err(Error::Target);
        }

        // Turn the data from json into bytes and copy into buf
        let data = response.data.unwrap();

        let hex_strings = serde_json::from_value::<Vec<String>>(data).map_err(|e| {
            warn!("Failed to convert JSON to Vec<String>: {e}");
            Error::Target
        })?;

        let byte_offset = (addr - start_aligned) as usize;
        for (i, hex_str) in hex_strings.iter().enumerate() {
            let value = u32::from_str_radix(hex_str.trim_start_matches("0x"), 16).map_err(|e| {
                warn!("Failed to parse hex string '{hex_str}': {e}");
                Error::Target
            })?;
            let word_bytes = value.to_le_bytes();

            for (j, &byte) in word_bytes.iter().enumerate() {
                let global_idx = i * 4 + j;
                if global_idx >= byte_offset && global_idx - byte_offset < buf.len() {
                    buf[global_idx - byte_offset] = byte;
                }
            }
        }

        Ok(())
    }

    fn update_base_address(&mut self, _new_base: u32) {
        // No-op as we does not have a concept of "base address"
    }
}

/// Writer instance used by custom Firmware implementations to write data to
/// the Target.
///
/// Unlike the Reader implementation, the writer MUST be used with 4-byte
/// aligned writes only.
pub struct FirmwareWriter {
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
    target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
}

impl FirmwareWriter {
    fn new(
        target_sender: Sender<
            'static,
            CriticalSectionRawMutex,
            TargetRequest,
            REQUEST_CHANNEL_SIZE,
        >,
        target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
    ) -> Self {
        Self {
            target_sender,
            target_response_signal,
        }
    }
}

impl Writer for FirmwareWriter {
    type Error = Error;

    async fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error> {
        trace!(
            "Info:  FirmwareWriter writing {:#010X} length {}",
            addr,
            data.len()
        );

        // Verify 4-byte alignment as required
        if addr & 3 != 0 {
            warn!("Write address {addr:#010X} is not 4-byte aligned");
            return Err(Error::NotAligned);
        }
        if data.len() & 3 != 0 {
            warn!(
                "Write data length {} is not a multiple of 4 bytes",
                data.len()
            );
            return Err(Error::NotAligned);
        }

        // Convert bytes to hex strings
        let hex_strings: Vec<String> = data
            .chunks_exact(4)
            .map(|chunk| {
                let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                format!("{word:#010X}")
            })
            .collect();

        // Build a Target request
        let addr_str = format!("{addr:#010X}");

        let command = TargetCommand::WriteMemBulk {
            addr: addr_str,
            data: hex_strings,
        };
        let request = TargetRequest {
            command,
            response_signal: self.target_response_signal,
        };

        trace!("Info:  FirmwareWriter send command: {:?}", request.command);

        self.target_sender.send(request).await;

        // Wait for the response
        let response = self.target_response_signal.wait().await;

        trace!("Info:  FirmwareWriter got response: {response:?}");

        // Handle errors
        if let Some(error) = response.error {
            warn!("Failed to write memory to target: {error}");
            return Err(Error::Target);
        }

        Ok(())
    }

    fn update_base_address(&mut self, _new_base: u32) {
        // No-op as we does not have a concept of "base address"
    }
}
