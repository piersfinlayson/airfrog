//! airfrog - One ROM Lab Firmware handling

// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

use airfrog_core::Mcu;
use airfrog_rpc::io::Reader;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use async_trait::async_trait;
use serde_json::Value;

use sdrr_fw_parser::{LabParser, OneRomLab};

use crate::firmware::Error;
use crate::firmware::types::{Decoder, Firmware, FirmwareType, WwwButton};
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
impl<R: Reader + 'static> Decoder<R> for OneRomLabDecoder<R> {
    fn fw_type(&self) -> FirmwareType {
        FIRMWARE_TYPE
    }

    async fn detect(&self, mcu: &Mcu, reader: &mut R) -> Option<FirmwareType> {
        if !mcu.is_stm32f4() {
            return None;
        }

        // Use the One ROM Lab firmware parser to detect One ROM Lab
        if self.parser(mcu, reader).detect().await {
            Some(FIRMWARE_TYPE)
        } else {
            None
        }
    }

    async fn decode(&self, mcu: &Mcu, reader: &mut R) -> Result<Box<dyn Firmware<R>>, Error> {
        if !mcu.is_stm32f4() {
            return Err(Error::UnknownFirmware);
        }

        // Use the One ROM Lab firmware parser to retrieve the firmware
        // information
        let lab = self.parser(mcu, reader).parse().await;

        Ok(Box::new(OneRomLabFirmware::new(lab)))
    }
}

pub struct OneRomLabFirmware<R: Reader> {
    _marker: core::marker::PhantomData<R>,
    info: OneRomLab,
}

#[async_trait(?Send)]
impl<R: Reader + 'static> Firmware<R> for OneRomLabFirmware<R> {
    fn fw_type(&self) -> FirmwareType {
        FIRMWARE_TYPE
    }

    fn rtt_cb_address(&self) -> Option<u32> {
        #[allow(clippy::bind_instead_of_map)]
        self.info
            .flash
            .as_ref()
            .and_then(|flash| Some(flash.rtt_ptr))
    }

    fn get_summary_kvp(&self) -> Result<Vec<(String, String)>, Error> {
        let mut kvp = Vec::new();

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
                "ROM Data Pointer".to_string(),
                format!("{:#010X}", ram.rom_data_ptr),
            ));
        } else {
            kvp.push(("ROM Data Pointer".to_string(), "Unknown".to_string()));
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
        let rom_data_ptr = ram.map_or(0, |r| r.rom_data_ptr);
        let rtt_ptr = flash.map_or(0, |f| f.rtt_ptr);
        let rpc_cmd_channel_ptr = ram.map_or(0, |r| r.rpc_cmd_channel_ptr);
        let rpc_rsp_channel_ptr = ram.map_or(0, |r| r.rpc_rsp_channel_ptr);

        let html = format!(
            r#"
<div class="card">
<h2>One ROM Lab</h2>
<table class="device-info">
<tr>
<td class="label-col"><strong>Firmware:</strong></td>
<td>One ROM Lab</td>
</tr>
<tr>
<td class="label-col"><strong>Version:</strong></td>
<td>V{major_version}.{minor_version}.{patch_version}</td>
</tr>
<tr>
<td class="label-col"><strong>Hardware:</strong></td>
<td>{hardware}</td>
</tr>
<tr>
<td class="label-col"><strong>ROM data:</strong></td>
<td>{rom_data_ptr:#010X}</td>
</tr>
<tr>
<td class="label-col"><strong>RTT data:</strong></td>
<td>{rtt_ptr:#010X}</td>
</tr>
<tr>
<td class="label-col"><strong>RPC Command Channel:</strong></td>
<td>{rpc_cmd_channel_ptr:#010X}</td>
</tr>
<tr>
<td class="label-col"><strong>RPC Response Channel:</strong></td>
<td>{rpc_rsp_channel_ptr:#010X}</td>
</tr>
</table>
</div>
"#,
        );

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
        _method: Method,
        _path: String,
        _body: Option<Value>,
        _reader: &mut R,
    ) -> Result<(StatusCode, Option<Value>), Error> {
        Err(Error::NotImplemented)
    }

    async fn handle_www(
        &self,
        method: Method,
        path: String,
        body: Option<String>,
        reader: &mut R,
    ) -> Result<(StatusCode, Option<String>), Error> {
        match path.as_str() {
            "read_rom" => self.handle_read_rom(method, body, reader).await,
            _ => Err(Error::NotImplemented),
        }
    }
}

impl<R: Reader + 'static> OneRomLabFirmware<R> {
    fn new(info: OneRomLab) -> Self {
        Self {
            info,
            _marker: core::marker::PhantomData,
        }
    }

    async fn handle_read_rom(
        &self,
        method: Method,
        body: Option<String>,
        _reader: &mut R,
    ) -> Result<(StatusCode, Option<String>), Error> {
        if method != Method::Get {
            return Err(Error::NotImplemented);
        }

        if body.is_none() {
            return Err(Error::NotImplemented);
        }

        let html = r#"<h1>Read ROM</h1>
<div class="card">
<p>Read ROM functionality will go here
<br/>
<br/>
<br/>
<br/>
<br/>
</div>
"#
        .to_string();

        Ok((StatusCode::Ok, Some(html)))
    }
}
