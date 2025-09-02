// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - SWD target objects and routines

#[cfg(any(feature = "www", feature = "rest"))]
use alloc::format;
#[cfg(any(feature = "www", feature = "rest"))]
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use core::fmt;
use embassy_futures::select::{Either, select};
#[cfg(feature = "bin-api")]
use embassy_net::tcp::TcpSocket;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Timer};
use esp_hal::gpio::{InputPin, OutputPin};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use static_cell::make_static;

#[cfg(any(feature = "rest", feature = "www"))]
use airfrog_bin::MAX_WORD_COUNT;
#[cfg(any(feature = "www", feature = "rest"))]
use airfrog_core::Mcu;
use airfrog_rpc::io::Reader;
use airfrog_swd::SwdError;
use airfrog_swd::debug::DebugInterface;
use airfrog_swd::protocol::Speed;
#[cfg(any(feature = "rest", feature = "www"))]
use airfrog_swd::protocol::{LineState, Version};

use crate::AirfrogError;
#[cfg(any(feature = "www", feature = "rest"))]
use crate::ErrorKind;

use crate::firmware::{Control as FirmwareControl, fw_control};

#[cfg(feature = "bin-api")]
use airfrog_swd::bin;

use crate::config::CONFIG;

pub(crate) mod request;
pub(crate) mod response;

pub(crate) use request::{Command, Request};
pub(crate) use response::{Response, Status};

/// Number of requests to this Target object that can be queued.
pub const REQUEST_CHANNEL_SIZE: usize = 2;

// Timers for Target task
const TARGET_KEEPALIVE_DURATION: Duration = Duration::from_millis(1000);
const TARGET_RECONNECT_DURATION: Duration = Duration::from_millis(1000);
const TARGET_RECONNECT_LOG_INTERVAL: u32 = 100;

// Port for the binary API
#[cfg(feature = "bin-api")]
const BIN_API_PORT: u16 = airfrog_bin::PORT;

// Size of the TCP RX and TX buffers for the binary API socket
#[cfg(feature = "bin-api")]
pub const BIN_API_TCP_RX_BUF_SIZE: usize = 4096;
#[cfg(feature = "bin-api")]
pub const BIN_API_TCP_TX_BUF_SIZE: usize = 4096;

/// Task to run SWD operations
#[embassy_executor::task]
pub(crate) async fn task(
    target: &'static mut Target<'static>,
    _stack: Option<embassy_net::Stack<'static>>,
) {
    info!("Exec:  Target task started");

    #[cfg(feature = "bin-api")]
    let rx_buffer = make_static!([0; BIN_API_TCP_RX_BUF_SIZE]);
    #[cfg(feature = "bin-api")]
    let tx_buffer = make_static!([0; BIN_API_TCP_TX_BUF_SIZE]);
    #[cfg(feature = "bin-api")]
    let stack = _stack.expect("Error: no networking for bin-api");
    #[cfg(feature = "bin-api")]
    let mut bin_api_socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
    #[cfg(feature = "bin-api")]
    info!("Exec:  Binary API started on port {BIN_API_PORT}");

    let mut reconnect_count: u32 = 0;
    loop {
        // Figure out how long select should wait for.
        let dur = if !target.is_connected().await {
            TARGET_RECONNECT_DURATION
        } else {
            TARGET_KEEPALIVE_DURATION
        };

        // Set up the future based on whether we want a binary API or not
        #[cfg(feature = "bin-api")]
        let accept_future = bin_api_socket.accept(BIN_API_PORT);
        #[cfg(feature = "bin-api")]
        let first_sel = select(target.request_receiver.receive(), accept_future);

        #[cfg(not(feature = "bin-api"))]
        let first_sel = target.request_receiver.receive();

        match select(first_sel, Timer::after(dur)).await {
            Either::First(req_acc) => {
                #[cfg(feature = "bin-api")]
                match req_acc {
                    Either::First(request) => {
                        // Request from httpd - handle it, respond, return
                        target.handle_request(request).await;
                    }
                    Either::Second(accept) => {
                        match accept {
                            Ok(()) => {
                                // Continues until a connection drops, or no activity
                                // occurs for a period of time
                                let mut bin_api = bin::Api::default();
                                bin_api.serve(&mut target.swd, &mut bin_api_socket).await;

                                // Now we've done some binary API serving,
                                // try to reset the target.
                                target.connect().await.ok();
                            }
                            Err(e) => {
                                warn!("Error: Failed to accept binary API connection: {e:?}");
                            }
                        }
                    }
                }
                #[cfg(not(feature = "bin-api"))]
                target.handle_request(req_acc).await;
            }
            Either::Second(_) => {
                if target.is_connected().await {
                    if target.refresh() {
                        target.do_refresh().await;
                    } else if target.keepalive() {
                        target.do_keepalive().await;
                    }
                } else if !target.is_connected().await && target.auto_connect() {
                    reconnect_count += 1;
                    if reconnect_count == 1
                        || reconnect_count.is_multiple_of(TARGET_RECONNECT_LOG_INTERVAL)
                    {
                        info!("Note:  Target not connected - connection attempt {reconnect_count}");
                    }
                    target.connect().await.ok();
                    if target.is_connected().await {
                        reconnect_count = 0;
                    }
                }
            }
        }
    }
}

/// Target Settings
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(crate) struct Settings {
    pub speed: Speed,
    pub auto_connect: bool,
    pub keepalive: bool,
    pub refresh: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speed: Speed::Turbo,
            auto_connect: true,
            keepalive: true,
            refresh: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum SettingsSource {
    Flash,
    Runtime,
}

/// An SWD target
pub(crate) struct Target<'a> {
    swd: DebugInterface<'a>,
    request_receiver: Receiver<'static, CriticalSectionRawMutex, Request, REQUEST_CHANNEL_SIZE>,
    request_sender: Sender<'static, CriticalSectionRawMutex, Request, REQUEST_CHANNEL_SIZE>,
    settings: Settings,
}

impl<'a> Target<'a> {
    pub(crate) fn new(
        settings: Settings,
        swdio_pin: impl InputPin + OutputPin + fmt::Debug + 'a,
        swclk_pin: impl OutputPin + fmt::Debug + 'a,
    ) -> Target<'a> {
        debug!("Exec:  Create SWD interface");
        let mut swd = DebugInterface::from_pins(swdio_pin, swclk_pin);

        // Set the speed here
        swd.swd_if().set_swd_speed(settings.speed);

        // Create the channel for requests
        let channel = make_static!(Channel::new());
        let request_receiver = channel.receiver();
        let request_sender = channel.sender();

        Self {
            swd,
            request_receiver,
            request_sender,
            settings,
        }
    }

    pub(crate) fn request_sender(
        &self,
    ) -> Sender<'static, CriticalSectionRawMutex, Request, REQUEST_CHANNEL_SIZE> {
        self.request_sender
    }

    async fn is_connected(&mut self) -> bool {
        let connected = self.swd.swd_if().is_connected();

        if !connected {
            // Stop the Firmware Task
            fw_control(FirmwareControl::Stop);
        }
        connected
    }

    async fn connect(&mut self) -> Result<(), SwdError> {
        self.swd.reset_swd_target().await?;
        let mcu = if let Some(mcu) = self.swd.mcu() {
            mcu
        } else {
            warn!("Error: Connected to target via SWD, but no MCU details");
            return Err(SwdError::NotReady);
        };
        info!("OK:    Target connected {self}");

        // Start the firmware task
        fw_control(FirmwareControl::Start { mcu });

        Ok(())
    }

    async fn do_keepalive(&mut self) -> () {
        if let Err(e) = self.swd.swd_if().keepalive().await {
            warn!("Note:  Target keepalive failed: {e}");

            // Stop the Firmware Task
            fw_control(FirmwareControl::Stop);
        }
    }

    async fn do_refresh(&mut self) -> () {
        debug!("Refreshing target firmware/RAM information");
        if self.swd.reset_swd_target().await.is_err() {
            warn!("Note:  Target refresh failed - unable to reset SWD target");

            // Stop the Firmware Task
            fw_control(FirmwareControl::Stop);
        };
    }

    fn refresh(&self) -> bool {
        self.settings.refresh
    }

    fn auto_connect(&self) -> bool {
        self.settings.auto_connect
    }

    fn keepalive(&self) -> bool {
        self.settings.keepalive
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    fn set_refresh(&mut self, refresh: bool) {
        self.settings.refresh = refresh;
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    fn set_auto_connect(&mut self, auto_connect: bool) {
        self.settings.auto_connect = auto_connect;
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    fn set_keepalive(&mut self, keepalive: bool) {
        self.settings.keepalive = keepalive;
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    fn get_detailed_info(&mut self) -> Result<DetailedInfo, SwdError> {
        let mcu = self.swd.mcu().ok_or(SwdError::NotReady)?;
        let idcode = self.swd.idcode().ok_or(SwdError::NotReady)?;

        // Check MEM-AP to get IDR
        let idr = self.swd.swd_if().idr();
        let mem_ap_idr = if let Some(idr) = idr {
            format!("{idr}")
        } else {
            "Unknown".to_string()
        };

        match mcu {
            Mcu::Stm32(stm_details) => {
                let stm_mcu = stm_details.mcu();
                let uid = stm_details.uid();
                let unique_id = if let Some(uid) = uid {
                    let bytes = uid.raw();
                    Some(format!(
                        "0x{:08X}{:08X}{:08X}",
                        bytes[0], bytes[1], bytes[2]
                    ))
                } else {
                    None
                };
                Ok(DetailedInfo {
                    idcode: format!("{idcode}"),
                    mcu_family: format!("{:?}", stm_mcu.family()),
                    mcu_line: stm_mcu.line().to_string(),
                    mcu_device_id: format!("0x{:03X}", stm_mcu.device_id()),
                    mcu_revision: stm_mcu.revision_str().to_string(),
                    flash_size_kb: stm_details.flash_size_kb().map(|fs| fs.size_kb()),
                    unique_id,
                    mem_ap_idr,
                })
            }
            Mcu::Rp(rp_details) => {
                let rp_line = rp_details.line();
                Ok(DetailedInfo {
                    idcode: format!("{idcode}"),
                    mcu_family: "Raspberry Pi".to_string(),
                    mcu_line: rp_line.to_string(),
                    mcu_device_id: "n/a".to_string(),
                    mcu_revision: "n/a".to_string(),
                    flash_size_kb: None,
                    unique_id: None,
                    mem_ap_idr,
                })
            }
            Mcu::Unknown(_) => Ok(DetailedInfo {
                idcode: format!("{idcode}"),
                mcu_family: "unknown".to_string(),
                mcu_line: "unknown".to_string(),
                mcu_device_id: "n/a".to_string(),
                mcu_revision: "n/a".to_string(),
                flash_size_kb: None,
                unique_id: None,
                mem_ap_idr,
            }),
        }
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    async fn clear_errors(&mut self) -> Result<(), SwdError> {
        self.swd.swd_if().clear_errors().await
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    async fn get_error_states(&mut self) -> Result<ErrorStates, SwdError> {
        let ctrl_stat = self.swd.swd_if().read_ctrl_stat().await?;

        Ok(ErrorStates {
            stkerr: ctrl_stat.stickyerr(),
            stkcmp: ctrl_stat.stickycmp(),
            wderr: ctrl_stat.wdataerr(),
            orunerr: ctrl_stat.stickyorun(),
            readok: ctrl_stat.readok(),
        })
    }

    #[cfg(not(feature = "httpd"))]
    async fn handle_request(&mut self, request: Request) {
        warn!(
            "Error: HTTP server not enabled - target request ignored: {:?}",
            request.command
        );
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    fn set_speed(&mut self, speed: Speed) {
        trace!("Setting SWD speed to {speed:?}");
        self.settings.speed = speed;
        self.swd.swd_if().set_swd_speed(speed);
    }

    #[cfg(any(feature = "rest", feature = "www"))]
    fn get_status(&mut self) -> Status {
        Status {
            connected: self.swd.swd_if().is_connected(),
            version: self.swd.swd_if().check_version().ok(),
            idcode: self.swd.idcode().map(|id| format!("{id}")),
            mcu: self.swd.mcu().map(|mcu| format!("{mcu}")),
            settings: self.settings,
        }
    }
}

#[cfg(feature = "httpd")]
// Handles the various REST requests
impl<'a> Target<'a> {
    // Main incoming request handler
    //
    // Receives commands from Httpd, handles them, and responds
    async fn handle_request(&mut self, request: Request) {
        trace!("Handling request: {:?}", request.command);
        let response = match request.command {
            Command::GetStatus => self.rest_get_status(),
            Command::Reset => self.rest_reset().await,
            Command::GetDetails => self.rest_get_details(),
            Command::ClearErrors => self.rest_clear_errors().await,
            Command::GetErrors => self.rest_get_errors().await,
            Command::ReadMem { addr } => self.rest_read_mem(addr).await,
            Command::WriteMem { addr, data } => self.rest_write_mem(addr, data).await,
            Command::ReadMemBulk { addr, count } => self.rest_read_mem_bulk(addr, count).await,
            Command::WriteMemBulk { addr, data } => self.rest_write_mem_bulk(addr, data).await,
            Command::UnlockFlash => self.rest_unlock_flash().await,
            Command::LockFlash => self.rest_lock_flash().await,
            Command::EraseSector { sector } => self.rest_erase_sector(sector).await,
            Command::EraseAll => self.rest_erase_all().await,
            Command::WriteFlashWord { addr, data } => self.rest_write_flash_word(addr, data).await,
            Command::WriteFlashBulk { addr, data } => self.rest_write_flash_bulk(addr, data).await,
            Command::GetSpeed => self.rest_get_speed(),
            Command::SetSpeed { speed } => self.rest_set_speed(speed).await,
            Command::RawReset => self.rest_raw_reset().await,
            Command::RawReadDpReg { register } => self.rest_raw_read_dp_reg(register).await,
            Command::RawWriteDpReg { register, data } => {
                self.rest_raw_write_dp_reg(register, data).await
            }
            Command::RawReadApReg { ap_index, register } => {
                self.rest_raw_read_ap_reg(ap_index, register).await
            }
            Command::RawWriteApReg {
                ap_index,
                register,
                data,
            } => self.rest_raw_write_ap_reg(ap_index, register, data).await,
            Command::RawBulkReadApReg {
                ap_index,
                register,
                count,
            } => {
                self.rest_raw_bulk_read_ap_reg(ap_index, register, count)
                    .await
            }
            Command::RawBulkWriteApReg {
                ap_index,
                register,
                count,
                data,
            } => {
                self.rest_raw_bulk_write_ap_reg(ap_index, register, count, data)
                    .await
            }
            Command::Clock {
                level,
                post_level,
                count,
            } => self.rest_raw_clock(level, post_level, count),
            Command::UpdateSettings { source, settings } => {
                self.update_settings(source, settings).await
            }
        }
        .unwrap_or_else(|e| e.into());
        request.response_signal.signal(response);
    }

    // Helper functions
    fn rest_get_word(word_str: &str) -> Result<u32, AirfrogError> {
        match u32::from_str_radix(word_str.trim_start_matches("0x"), 16) {
            Ok(word) => Ok(word),
            Err(_) => {
                debug!("Invalid address format: {word_str}");
                Err(AirfrogError::Airfrog(ErrorKind::InvalidBody))
            }
        }
    }

    fn rest_get_addr(addr_str: &str) -> Result<u32, AirfrogError> {
        // If lookup fails for address word, it's path not body
        let addr = Self::rest_get_word(addr_str)
            .map_err(|_| AirfrogError::Airfrog(ErrorKind::InvalidPath))?;

        if !addr.is_multiple_of(4) {
            debug!("Flash address {addr} is not word-aligned");
            return Err(AirfrogError::Airfrog(ErrorKind::InvalidBody));
        }

        Ok(addr)
    }

    fn rest_get_data(data_str: &str) -> Result<u32, AirfrogError> {
        // If lookup fails for data word, it's body - unnecessary so
        // long as rest_get_word() continues to return InvalidBody
        Self::rest_get_word(data_str).map_err(|_| AirfrogError::Airfrog(ErrorKind::InvalidBody))
    }

    fn rest_check_data_len(count: usize) -> Result<(), AirfrogError> {
        if count > MAX_WORD_COUNT as usize {
            debug!("Bulk data request too large: {count}");
            Err(AirfrogError::Airfrog(ErrorKind::TooLarge))
        } else {
            Ok(())
        }
    }

    fn rest_get_bulk_data(data_strs: Vec<String>) -> Result<Vec<u32>, AirfrogError> {
        Self::rest_check_data_len(data_strs.len())?;

        let mut data_words = Vec::new();
        for data_str in data_strs {
            let word = Self::rest_get_word(&data_str)?;
            data_words.push(word);
        }

        Ok(data_words)
    }

    // Raw register validation helpers
    fn rest_get_dp_register(register_str: &str) -> Result<u8, AirfrogError> {
        let register = Self::rest_get_word(register_str)?;
        if register > 0xF {
            debug!("DP register address {register:02X} out of range");
            return Err(AirfrogError::Airfrog(ErrorKind::InvalidPath));
        }
        Ok(register as u8)
    }

    fn rest_get_ap_index(ap_index_str: &str) -> Result<u8, AirfrogError> {
        let ap_index = Self::rest_get_word(ap_index_str)?;
        if ap_index > 0xFF {
            debug!("AP index {ap_index:02X} out of range");
            return Err(AirfrogError::Airfrog(ErrorKind::InvalidPath));
        }
        Ok(ap_index as u8)
    }

    fn rest_get_ap_register(register_str: &str) -> Result<u8, AirfrogError> {
        let register = Self::rest_get_word(register_str)?;
        Ok(register as u8)
    }

    // REST API functions
    fn rest_get_status(&mut self) -> Result<Response, AirfrogError> {
        trace!("Getting target status");
        Ok(Response::default().with_swd_status(self.get_status()))
    }

    async fn rest_reset(&mut self) -> Result<Response, AirfrogError> {
        info!("Exec:  Reset target");
        if !self.auto_connect() && !self.keepalive() && !self.refresh() {
            info!("Exec:  Re-enabling auto-connect, keepalive and refresh");
        }
        self.set_auto_connect(true);
        self.set_keepalive(true);
        self.set_refresh(true);

        match self.connect().await {
            Ok(()) => Ok(Response::default().with_swd_status(self.get_status())),
            Err(e) => Err(e.into()),
        }
    }

    fn rest_get_details(&mut self) -> Result<Response, AirfrogError> {
        trace!("Exec:  Getting target details");
        match self.get_detailed_info() {
            Ok(details) => {
                let data = match serde_json::to_value(details) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Error: Failed to serialize target details: {e}");
                        return Err(AirfrogError::Airfrog(ErrorKind::InternalServerError));
                    }
                };
                Ok(Response::default()
                    .with_swd_status(self.get_status())
                    .with_data(data))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_clear_errors(&mut self) -> Result<Response, AirfrogError> {
        trace!("Clearing SWD errors");
        match self.clear_errors().await {
            Ok(()) => Ok(Response::default().with_swd_status(self.get_status())),
            Err(e) => Ok(Response::from(e).with_swd_status(self.get_status())),
        }
    }

    async fn rest_get_errors(&mut self) -> Result<Response, AirfrogError> {
        trace!("Getting SWD error states");
        match self.get_error_states().await {
            Ok(error_states) => {
                let data = match serde_json::to_value(error_states) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("Error: Failed to serialize error states: {e}");
                        return Err(AirfrogError::Airfrog(ErrorKind::InternalServerError));
                    }
                };
                Ok(Response::default()
                    .with_swd_status(self.get_status())
                    .with_data(data))
            }
            Err(e) => Ok(Response::from(e).with_swd_status(self.get_status())),
        }
    }

    async fn rest_read_mem(&mut self, addr_str: String) -> Result<Response, AirfrogError> {
        trace!("Reading memory at {addr_str}");
        let addr = Self::rest_get_addr(addr_str.as_str())?;

        match self.swd.read_mem(addr).await {
            Ok(data) => {
                Ok(Response::default().with_data(serde_json::json!(format!("0x{:08X}", data))))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_write_mem(
        &mut self,
        addr_str: String,
        data_str: String,
    ) -> Result<Response, AirfrogError> {
        trace!("Writing {data_str} to memory at {addr_str}");
        let addr = Self::rest_get_addr(addr_str.as_str())?;
        let data = Self::rest_get_data(&data_str)?;

        match self.swd.write_mem(addr, data).await {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_read_mem_bulk(
        &mut self,
        addr_str: String,
        count: usize,
    ) -> Result<Response, AirfrogError> {
        trace!("Reading {count} words from memory at {addr_str}");
        let addr = Self::rest_get_addr(&addr_str)?;

        Self::rest_check_data_len(count)?;

        let mut buf = vec![0u32; count];
        match self.swd.read_mem_bulk(addr, &mut buf, true).await {
            Ok(()) => {
                let hex_data: Vec<String> =
                    buf.iter().map(|&word| format!("0x{word:08X}")).collect();
                Ok(Response::default().with_data(serde_json::json!(hex_data)))
            }
            Err((e, _)) => Err(e.into()),
        }
    }

    async fn rest_write_mem_bulk(
        &mut self,
        addr_str: String,
        data_strs: Vec<String>,
    ) -> Result<Response, AirfrogError> {
        trace!("Writing {} words to memory at {addr_str}", data_strs.len(),);
        let addr = Self::rest_get_addr(&addr_str)?;
        let data_words = Self::rest_get_bulk_data(data_strs)?;

        match self.swd.write_mem_bulk(addr, &data_words, true).await {
            Ok(()) => Ok(Response::default()),
            Err((e, _)) => Err(e.into()),
        }
    }

    async fn rest_unlock_flash(&mut self) -> Result<Response, AirfrogError> {
        trace!("Unlocking flash");
        match self.swd.unlock_flash().await {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_lock_flash(&mut self) -> Result<Response, AirfrogError> {
        trace!("Locking flash");
        match self.swd.lock_flash().await {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_erase_sector(&mut self, sector: u32) -> Result<Response, AirfrogError> {
        trace!("Erasing flash sector {sector}");
        match self.swd.erase_sector(sector).await {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_erase_all(&mut self) -> Result<Response, AirfrogError> {
        trace!("Erasing all flash sectors");
        match self.swd.erase_all().await {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_write_flash_word(
        &mut self,
        addr_str: String,
        data_str: String,
    ) -> Result<Response, AirfrogError> {
        trace!("Writing {data_str} to flash at {addr_str}");
        let addr = Self::rest_get_addr(&addr_str)?;
        let data = Self::rest_get_data(&data_str)?;

        match self.swd.write_flash_u32(addr, data).await {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_write_flash_bulk(
        &mut self,
        addr_str: String,
        data_strs: Vec<String>,
    ) -> Result<Response, AirfrogError> {
        trace!("Writing {} words to flash at {}", data_strs.len(), addr_str);
        let addr = Self::rest_get_addr(&addr_str)?;
        let data_words = Self::rest_get_bulk_data(data_strs)?;

        match self.swd.write_flash_bulk(addr, &data_words).await {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn rest_get_speed(&mut self) -> Result<Response, AirfrogError> {
        trace!("Getting SWD speed");
        let current_speed = self.swd.swd_if().swd_speed();
        self.settings.speed = current_speed;
        Ok(Response::default().with_speed(current_speed))
    }

    async fn rest_set_speed(&mut self, speed: Speed) -> Result<Response, AirfrogError> {
        trace!("Setting SWD speed to {speed:?}");
        self.set_speed(speed);
        Ok(Response::default())
    }

    async fn rest_raw_reset(&mut self) -> Result<Response, AirfrogError> {
        trace!("Performing raw reset");
        info!("Exec:  Disabling auto-connect keepalive and refresh");
        self.set_auto_connect(false);
        self.set_keepalive(false);
        self.set_refresh(false);

        // Try to connect as V1.  If that fails, try V2.  Multi-drop
        // is not attempted.
        if self.swd.swd_if().reset_target(Version::V1).await.is_ok() {
            Ok(Response::default())
        } else {
            self.swd
                .swd_if()
                .reset_target(Version::V2)
                .await
                .map(|_| Response::default())
                .map_err(|e| e.into())
        }
    }

    async fn rest_raw_read_dp_reg(
        &mut self,
        register_str: String,
    ) -> Result<Response, AirfrogError> {
        trace!("Reading DP register {register_str}");
        let register = Self::rest_get_dp_register(&register_str)?;

        match self.swd.swd_if().read_dp_register_raw(register).await {
            Ok(data) => {
                Ok(Response::default().with_data(serde_json::json!(format!("0x{:08X}", data))))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_raw_write_dp_reg(
        &mut self,
        register_str: String,
        data_str: String,
    ) -> Result<Response, AirfrogError> {
        trace!("Writing {data_str} to DP register {register_str}");
        let register = Self::rest_get_dp_register(&register_str)?;
        let data = Self::rest_get_data(&data_str)?;

        match self
            .swd
            .swd_if()
            .write_dp_register_raw(register, data)
            .await
        {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_raw_read_ap_reg(
        &mut self,
        ap_index_str: String,
        register_str: String,
    ) -> Result<Response, AirfrogError> {
        debug!("Reading AP {ap_index_str} register {register_str}");
        let ap_index = Self::rest_get_ap_index(&ap_index_str)?;
        let register = Self::rest_get_ap_register(&register_str)?;

        match self
            .swd
            .swd_if()
            .read_ap_register_raw(ap_index, register)
            .await
        {
            Ok(data) => {
                Ok(Response::default().with_data(serde_json::json!(format!("0x{:08X}", data))))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_raw_write_ap_reg(
        &mut self,
        ap_index_str: String,
        register_str: String,
        data_str: String,
    ) -> Result<Response, AirfrogError> {
        debug!("Writing {data_str} to AP {ap_index_str} register {register_str}");
        let ap_index = Self::rest_get_ap_index(&ap_index_str)?;
        let register = Self::rest_get_ap_register(&register_str)?;
        let data = Self::rest_get_data(&data_str)?;

        match self
            .swd
            .swd_if()
            .write_ap_register_raw(ap_index, register, data)
            .await
        {
            Ok(()) => Ok(Response::default()),
            Err(e) => Err(e.into()),
        }
    }

    async fn rest_raw_bulk_read_ap_reg(
        &mut self,
        ap_index_str: String,
        register_str: String,
        count: usize,
    ) -> Result<Response, AirfrogError> {
        debug!("Bulk reading {count} words from AP {ap_index_str} register {register_str}");
        let ap_index = Self::rest_get_ap_index(&ap_index_str)?;
        let register = Self::rest_get_ap_register(&register_str)?;

        Self::rest_check_data_len(count)?;

        // Set auto-increment mode
        self.swd.swd_if().set_addr_inc(true).await?;

        let mut buf = vec![0u32; count];
        let result = match self
            .swd
            .swd_if()
            .read_ap_register_raw_bulk(ap_index, register, &mut buf)
            .await
        {
            Ok(()) => {
                let hex_data: Vec<String> =
                    buf.iter().map(|&word| format!("0x{word:08X}")).collect();
                Ok(Response::default().with_data(serde_json::json!(hex_data)))
            }
            Err((e, _count)) => Err(e.into()),
        };

        // Unset auto-increment mode
        self.swd.swd_if().set_addr_inc(false).await?;

        result
    }

    async fn rest_raw_bulk_write_ap_reg(
        &mut self,
        ap_index_str: String,
        register_str: String,
        count: usize,
        data_strs: Vec<String>,
    ) -> Result<Response, AirfrogError> {
        debug!("Bulk writing {count} words to AP {ap_index_str} register {register_str}");
        let ap_index = Self::rest_get_ap_index(&ap_index_str)?;
        let register = Self::rest_get_ap_register(&register_str)?;
        let data_words = Self::rest_get_bulk_data(data_strs)?;

        Self::rest_check_data_len(count)?;

        // Set auto-increment mode
        self.swd.swd_if().set_addr_inc(true).await?;

        let result = match self
            .swd
            .swd_if()
            .write_ap_register_raw_bulk(ap_index, register, &data_words)
            .await
        {
            Ok(()) => Ok(Response::default()),
            Err((e, _count)) => Err(e.into()),
        };

        // Unset auto-increment mode
        self.swd.swd_if().set_addr_inc(false).await?;

        result
    }

    fn rest_raw_clock(
        &mut self,
        level: LineState,
        post_level: LineState,
        count: u32,
    ) -> Result<Response, AirfrogError> {
        trace!(
            "Performing raw clock with level {level:?}, post-level {post_level:?}, count {count}"
        );

        self.swd.swd_if().clock_raw(level, post_level, count);

        Ok(Response::default())
    }
}

// WWW commands
#[cfg(any(feature = "www", feature = "rest"))]
impl<'a> Target<'a> {
    async fn update_settings(
        &mut self,
        source: SettingsSource,
        settings: Settings,
    ) -> Result<Response, AirfrogError> {
        trace!("Updating target settings: {settings:?}");
        if source == SettingsSource::Runtime {
            self.set_speed(settings.speed);
            self.set_auto_connect(settings.auto_connect);
            self.set_keepalive(settings.keepalive);
            self.set_refresh(settings.refresh);
        } else {
            let mut config = CONFIG.get().await.lock().await;
            config.swd = settings.into();
            config.update_flash().await;
        }
        Ok(Response::default())
    }
}

impl<'a> fmt::Display for Target<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            if let Some(idcode) = self.swd.idcode() {
                write!(f, "{idcode}")?;
            } else {
                write!(f, "Target not connected")?;
            }
            if let Some(mcu) = self.swd.mcu() {
                write!(f, ", {mcu:#}")?;
            }
            Ok(())
        } else {
            if let Some(mcu) = self.swd.mcu() {
                write!(f, "{mcu}")?;
            } else {
                write!(f, "MCU not found")?;
            }
            Ok(())
        }
    }
}

// Implements the Reader trait for reading memory from the target.  This is
// used by the `firmware` module to read the firmware from the target.
impl<'a> Reader for &mut Target<'a> {
    type Error = AirfrogError;

    async fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        let offset = (addr & 3) as usize;
        let word_count = (offset + buf.len()).div_ceil(4);
        let mut words = vec![0u32; word_count];

        self.swd
            .read_mem_bulk(addr & !3, &mut words, true)
            .await
            .map_err(|(e, _)| Self::Error::from(e))?;

        let bytes: Vec<u8> = words.into_iter().flat_map(|w| w.to_le_bytes()).collect();

        let offset = (addr & 3) as usize;
        buf.copy_from_slice(&bytes[offset..offset + buf.len()]);
        Ok(())
    }

    fn update_base_address(&mut self, _new_base: u32) {
        // No-op as Target does not have a concept of "base address"
    }
}

#[cfg(any(feature = "www", feature = "rest"))]
#[derive(serde::Serialize)]
pub struct DetailedInfo {
    pub idcode: String,
    pub mcu_family: String,
    pub mcu_line: String,
    pub mcu_device_id: String,
    pub mcu_revision: String,
    pub flash_size_kb: Option<u32>,
    pub unique_id: Option<String>,
    pub mem_ap_idr: String,
}

#[cfg(any(feature = "www", feature = "rest"))]
#[derive(serde::Serialize)]
pub struct ErrorStates {
    pub stkerr: bool,
    pub stkcmp: bool,
    pub wderr: bool,
    pub orunerr: bool,
    pub readok: bool,
}
