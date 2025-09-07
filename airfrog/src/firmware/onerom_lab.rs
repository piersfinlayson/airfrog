//! airfrog - One ROM Lab Firmware handling

// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

use airfrog_core::Mcu;
use airfrog_rpc::Error as RpcError;
use airfrog_rpc::client::{AsyncDelay, AsyncRpcClient, RpcClientConfig};
use airfrog_rpc::io::{Reader, Writer};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use async_trait::async_trait;
use embassy_time::{Duration, Timer};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use serde_json::Value;

use onerom_protocol::Error as OneRomError;
use onerom_protocol::lab::{Command as RpcCommand, LabRomEntry};
use sdrr_fw_parser::{LabParser, OneRomLab};

use crate::firmware::Error;
use crate::firmware::assets::ONEROM_LAB_READROM_JS_PATH;
use crate::firmware::types::{Decoder, Firmware, FirmwareType, WwwButton};
use crate::http::html::build::HtmlBuilder;
use crate::http::{Method, StatusCode};

const FIRMWARE_TYPE: FirmwareType = FirmwareType::OneRomLab;

pub struct OneRomLabDecoder<R: Reader> {
    _marker: core::marker::PhantomData<R>,
}

impl<R: Reader> OneRomLabDecoder<R> {
    pub const fn new() -> Self {
        Self {
            _marker: core::marker::PhantomData,
        }
    }

    pub fn parser<'a>(&self, mcu: &Mcu, reader: &'a mut R) -> LabParser<'a, R> {
        let base_flash_address = match mcu {
            Mcu::Rp(_) => 0x1000_0000,
            Mcu::Stm32(_) => 0x0800_0000,
            _ => unreachable!(),
        };
        let base_ram_address = 0x2000_0000;
        LabParser::with_base_flash_address(reader, base_flash_address, base_ram_address)
    }
}

#[async_trait(?Send)]
impl<R: Reader + 'static, W: Writer + 'static> Decoder<R, W> for OneRomLabDecoder<R> {
    fn fw_type(&self) -> FirmwareType {
        FIRMWARE_TYPE
    }

    async fn detect(&self, mcu: &Mcu, reader: &mut R) -> Option<FirmwareType> {
        if !mcu.is_stm32f4() {
            return None;
        }

        debug!("Info:  Detecting One ROM Lab firmware...");

        // Use the One ROM Lab firmware parser to detect One ROM Lab
        if self.parser(mcu, reader).detect().await {
            Some(FIRMWARE_TYPE)
        } else {
            None
        }
    }

    async fn decode(&self, mcu: &Mcu, reader: &mut R) -> Result<Box<dyn Firmware<R, W>>, Error> {
        if !mcu.is_stm32f4() {
            return Err(Error::UnknownFirmware);
        }

        // Use the One ROM Lab firmware parser to retrieve the firmware
        // information
        let lab = self.parser(mcu, reader).parse().await;

        debug!("Info:  Decoded One ROM Lab firmware: {lab:?}");

        Ok(Box::new(OneRomLabFirmware::new(lab)))
    }
}

pub struct OneRomLabFirmware<R: Reader, W: Writer> {
    _r_marker: core::marker::PhantomData<R>,
    _w_marker: core::marker::PhantomData<W>,
    info: OneRomLab,
}

#[async_trait(?Send)]
impl<R: Reader + 'static, W: Writer + 'static> Firmware<R, W> for OneRomLabFirmware<R, W> {
    fn fw_type(&self) -> FirmwareType {
        FIRMWARE_TYPE
    }

    fn rtt_cb_address(&self) -> Option<u32> {
        #[allow(clippy::bind_instead_of_map)]
        self.info.flash.as_ref().and_then(|flash| {
            if flash.rtt_ptr != 0 {
                Some(flash.rtt_ptr)
            } else {
                None
            }
        })
    }

    fn get_summary_kvp(&self) -> Result<Vec<(String, String)>, Error> {
        let mut kvp = Vec::new();

        kvp.push(("Firmware".to_string(), "One ROM Lab".to_string()));

        // Get summary information from flash
        if let Some(flash) = &self.info.flash.as_ref() {
            kvp.push((
                "Version".to_string(),
                format!(
                    "v{}.{}.{}",
                    flash.major_version, flash.minor_version, flash.patch_version
                ),
            ));
            kvp.push(("Hardware Revision".to_string(), flash.hw_rev.to_string()));
            kvp.push(("Build Features".to_string(), flash.features.to_string()));
        } else {
            kvp.push(("Version".to_string(), "Unknown".to_string()));
            kvp.push(("Hardware Revision".to_string(), "Unknown".to_string()));
            kvp.push(("Build Features".to_string(), "Unknown".to_string()));
        }

        // Get summary information from RAM
        if let Some(ram) = &self.info.ram.as_ref() {
            kvp.push((
                "ROM Data".to_string(),
                format!("{:#010X}", ram.rom_data_ptr),
            ));
        } else {
            kvp.push(("ROM Data".to_string(), "Unknown".to_string()));
        }

        Ok(kvp)
    }

    fn get_full_html(&self) -> Result<(StatusCode, Option<String>), Error> {
        let flash = self.info.flash.as_ref();
        let ram = self.info.ram.as_ref();

        let major_version = flash.map_or("?", |f| f.major_version.as_str());
        let minor_version = flash.map_or("?", |f| f.minor_version.as_str());
        let patch_version = flash.map_or("?", |f| f.patch_version.as_str());
        let hardware = flash.map_or("?", |f| f.hw_rev.as_str());
        let features = flash.map_or("?", |f| f.features.as_str());
        let rom_data_ptr = ram.map_or(0, |r| r.rom_data_ptr);
        let rtt_ptr = flash.map_or(0, |f| f.rtt_ptr);
        let rpc_cmd_channel_ptr = ram.map_or(0, |r| r.rpc_cmd_channel_ptr);
        let rpc_rsp_channel_ptr = ram.map_or(0, |r| r.rpc_rsp_channel_ptr);
        let rpc_cmd_channel_size = ram.map_or(0, |r| r.rpc_cmd_channel_size);
        let rpc_rsp_channel_size = ram.map_or(0, |r| r.rpc_rsp_channel_size);

        let html = HtmlBuilder::new()
            .div()
            .class("card")
            .child(|card| {
                card.h2("One ROM Lab")
                    .with_table(Some("device-info"), |table| {
                        table
                            .row(|row| row.label_cell("Firmware:").cell("One ROM Lab"))
                            .row(|row| {
                                row.label_cell("Version:").cell(&format!(
                                    "V{major_version}.{minor_version}.{patch_version}",
                                ))
                            })
                            .row(|row| row.label_cell("Hardware:").cell(hardware))
                            .row(|row| row.label_cell("Features:").cell(features))
                            .row(|row| {
                                row.label_cell("ROM data:")
                                    .cell(&format!("{rom_data_ptr:#010X}"))
                            })
                            .row(|row| {
                                row.label_cell("RTT data:")
                                    .cell(&format!("{rtt_ptr:#010X}"))
                            })
                            .row(|row| {
                                row.label_cell("RPC Command Channel:")
                                    .cell(&format!("{rpc_cmd_channel_ptr:#010X}"))
                            })
                            .row(|row| {
                                row.label_cell("Channel Size:")
                                    .cell(&rpc_cmd_channel_size.to_string())
                            })
                            .row(|row| {
                                row.label_cell("RPC Response Channel:")
                                    .cell(&format!("{rpc_rsp_channel_ptr:#010X}"))
                            })
                            .row(|row| {
                                row.label_cell("Channel Size:")
                                    .cell(&rpc_rsp_channel_size.to_string())
                            })
                    })
            })
            .build();

        Ok((StatusCode::Ok, Some(html)))
    }

    fn get_buttons(&self) -> Result<Vec<WwwButton>, Error> {
        Ok(vec![WwwButton {
            name: "Read ROM".to_string(),
            path: "read_rom".to_string(),
        }])
    }

    async fn handle_rest(
        &self,
        method: Method,
        path: String,
        body: Option<Value>,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<(StatusCode, Option<Value>), Error> {
        match path.as_str() {
            "read_rom" => Ok(self
                .handle_rest_read_rom(method, body, reader, writer)
                .await?),
            _ => Err(Error::NotImplemented),
        }
    }

    async fn handle_www(
        &self,
        method: Method,
        path: String,
        body: Option<String>,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<(StatusCode, Option<String>), Error> {
        match path.as_str() {
            "read_rom" => Ok(self
                .handle_www_read_rom(method, body, reader, writer)
                .await?),
            _ => Err(Error::NotImplemented),
        }
    }
}

// Generic methods
impl<R: Reader + 'static, W: Writer + 'static> OneRomLabFirmware<R, W> {
    fn new(info: OneRomLab) -> Self {
        Self {
            info,
            _r_marker: core::marker::PhantomData,
            _w_marker: core::marker::PhantomData,
        }
    }

    // Used to retrieve configuration for the RPC client
    fn rpc_config(&self) -> Result<RpcClientConfig, CustomError> {
        let ram_info = self
            .info
            .ram
            .as_ref()
            .ok_or_else(|| CustomError::Custom("No RAM info available".to_string()))?;

        let cmd_ch_ptr = ram_info.rpc_cmd_channel_ptr;
        let rsp_ch_ptr = ram_info.rpc_rsp_channel_ptr;

        Ok(RpcClientConfig::FromTarget {
            cmd_ch_ptr,
            rsp_ch_ptr,
        })
    }
}

// An AsyncDelay implementation using embassy_time, for the AsyncRpcClient.
// It uses this as a timer between polls of the response channel.
struct Delay;
impl AsyncDelay for Delay {
    async fn delay() {
        Timer::after(Duration::from_millis(50)).await;
    }
}

// Type to make accessing the RPC Client simpler.
type RpcClient<'a, R, W> = AsyncRpcClient<'a, R, W, Delay>;

// WWW specific methods
impl<R: Reader + 'static, W: Writer + 'static> OneRomLabFirmware<R, W> {
    async fn handle_www_read_rom(
        &self,
        method: Method,
        body: Option<String>,
        _reader: &mut R,
        _writer: &mut W,
    ) -> Result<(StatusCode, Option<String>), Error> {
        // Arg checking
        if method != Method::Get {
            return Err(Error::NotImplemented)?;
        }
        if body.is_some() {
            return Err(Error::NotImplemented)?;
        }

        // Create the HTML using builder
        let html = HtmlBuilder::new()
            .h1("Read ROM")
            .div()
            .class("card")
            .child(|card| {
                card.div()
                    .child(|inner_div| inner_div.button("Read ROM", "readRom()"))
                    .br()
                    .div()
                    .id("rom-result")
                    .child(|result_div| {
                        result_div.with_table(Some("device-info"), |table| {
                            table.row(|row| {
                                row.with_width("200px")
                                    .label_cell("ROM")
                                    .cell("Not yet read")
                            })
                        })
                    })
            })
            .script_src(ONEROM_LAB_READROM_JS_PATH)
            .build();

        Ok((StatusCode::Ok, Some(html)))
    }
}

// REST specific methods
impl<R: Reader + 'static, W: Writer + 'static> OneRomLabFirmware<R, W> {
    async fn handle_rest_read_rom(
        &self,
        method: Method,
        body: Option<Value>,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<(StatusCode, Option<Value>), CustomError> {
        // Arg checking
        if method != Method::Post {
            return Err(Error::NotImplemented)?;
        }
        if body.is_some() {
            return Err(Error::NotImplemented)?;
        }

        // Send the ReadRom request via RPC to the target
        let mut rpc = RpcClient::new(reader, writer, self.rpc_config()?);
        let rsp = rpc.request(&RpcCommand::ReadRom.as_bytes()).await?;

        // Parse the response
        match LabRomEntry::from_buffer(&rsp) {
            Ok(rom_data) => {
                // Turn the ROM data into JSON
                let json = serde_json::to_value(rom_data).map_err(|e| {
                    CustomError::Custom(format!("ROM Data JSON serialization error: {e}"))
                })?;
                Ok((StatusCode::Ok, Some(json)))
            }
            Err(OneRomError::NoRom) => {
                // No ROM data present
                let json = serde_json::json!({"ROM": "Unknown or no ROM detected"});
                Ok((StatusCode::Ok, Some(json)))
            }
            Err(OneRomError::RomNotRecognised) => {
                // No ROM data present
                let json = serde_json::json!({"ROM": "Unrecognised"});
                Ok((StatusCode::Ok, Some(json)))
            }
            Err(e) => {
                // Some other error occurred
                error!("warn:  Error reading ROM data from target: {e:?}");
                let json = serde_json::json!({"Error": format!("{e:?}")});
                Ok((StatusCode::Ok, Some(json)))
            }
        }
    }

    /*
    async fn get_unrecognised_rom_data(&self, rpc: &mut RpcClient<'_, R, W>) -> Result<Vec<u8>, CustomError> {
        let mut rom_data = Vec::new();
        let mut offset = 0;

        loop {
            // Send the ReadRomData request via RPC to the target
            let cmd = RpcCommand::GetRawData { offset, length: 256 };
            let rsp = rpc.request(&cmd.as_bytes()).await?;

            if rsp.is_empty() {
                break; // No more data
            }

            rom_data.extend_from_slice(&rsp);
            offset += rsp.len() as u32;

            if rsp.len() < 256 {
                break; // Last chunk received
            }
        }

        Ok(rom_data)
    }
    */
}

// Need a custom error type so we can map between RpcError and (Firmware)Error
enum CustomError {
    FwError(Error),
    Custom(String),
}

impl From<String> for CustomError {
    fn from(err: String) -> Self {
        CustomError::Custom(err)
    }
}

impl From<RpcError> for CustomError {
    fn from(err: RpcError) -> Self {
        CustomError::Custom(format!("RPC Error: {err:?}"))
    }
}

impl From<Error> for CustomError {
    fn from(err: Error) -> Self {
        CustomError::FwError(err)
    }
}

impl From<CustomError> for Error {
    fn from(err: CustomError) -> Self {
        match err {
            CustomError::FwError(fw_err) => fw_err,
            CustomError::Custom(details) => Error::Custom(details),
        }
    }
}

impl From<OneRomError> for CustomError {
    fn from(err: OneRomError) -> Self {
        CustomError::Custom(format!("One ROM Lab Error: {err:?}"))
    }
}
