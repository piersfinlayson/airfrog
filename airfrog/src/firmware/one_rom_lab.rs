//! airfrog - One ROM Lab Firmware handling

// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};

use sdrr_fw_parser::{LabParser, Reader as OneRomReader, OneRomLab};

use crate::firmware::AF_FW_TYPE_KEY;
use crate::firmware::{FormatterError, FwError, FwHandler, FwHandlerInfo, FwInfo, JsonToHtml};
use airfrog_core::Mcu;

pub(crate) const AF_FW_TYPE: &str = "OneRomLab";

/// One ROM Lab firmware handler information
pub struct OneRomLabHandlerInfo;

impl FwHandlerInfo for OneRomLabHandlerInfo {
    fn name() -> &'static str {
        "One ROM Lab"
    }

    fn supports_mcu(mcu: &Mcu) -> bool {
        mcu.is_stm32f4()
    }
}

/// One ROM Lab firmware handler
pub struct OneRomHandler<R: OneRomReader> {
    parser: LabParser<R>,
}

impl<R: OneRomReader> FwHandler<R> for OneRomHandler<R> {
    fn new(reader: R) -> Self {
        let parser = LabParser::new(reader);
        Self { parser }
    }

    async fn detect(&mut self) -> bool {
        self.parser.detect().await
    }

    async fn parse_info(&mut self) ->  Result<Box<dyn FwInfo>, FwError<R::Error>> {
        let lab = self.parser.parse().await;
        Ok(Box::new(FwOneRomLab(lab)))
    }
}

pub struct FwOneRomLab(OneRomLab);

impl FwInfo for FwOneRomLab {
    fn summary(&self) -> serde_json::Value {
        let content = &self.0;
        match content.flash.as_ref() {
            Some(flash) => {
                let rom_data_ptr = if let Some(ram) = &content.ram {
                    ram.rom_data_ptr
                } else {
                    0
                };
                serde_json::json!({
                    "version": format!("v{}.{}.{}", flash.major_version, flash.minor_version, flash.patch_version),
                    "hw_rev": flash.hw_rev,
                    "rom_data_ptr": rom_data_ptr,
                })
            }
            None => {
                serde_json::json!({
                    "error": "No flash content found in One ROM Lab firmware"
                })
            }
        }
    }

    fn details(&self) -> serde_json::Value {
        let content = &self.0;
        let mut value = serde_json::json!(content);
        value[AF_FW_TYPE_KEY] = AF_FW_TYPE.into();
        value
    }
}

#[derive(Debug, Default)]
pub struct JsonToHtmler {}

impl JsonToHtmler {
    fn get_info(&self, data: serde_json::Value) -> Result<OneRomLab, FormatterError> {
        if !self.can_handle(&data) {
            return Err(FormatterError::JsonToHtml(
                "Unsupported firmware data".to_string(),
            ));
        }

        match serde_json::from_value(data) {
            Ok(info) => Ok(info),
            Err(e) => Err(FormatterError::JsonToHtml(format!(
                "Failed to parse One ROM Lab info: {e}"
            ))),
        }
    }
}

impl JsonToHtml for JsonToHtmler {
    fn can_handle(&self, data: &serde_json::Value) -> bool {
        data.get("_af_fw_type").is_some_and(|t| t == AF_FW_TYPE)
    }


    fn summary(&self, data: serde_json::Value) -> Result<String, FormatterError> {
        let info = self.get_info(data)?;
        let flash = info.flash;
        let ram = info.ram;

        let major_version = flash.as_ref().map_or("?", |f| f.major_version.as_str());
        let minor_version = flash.as_ref().map_or("?", |f| f.minor_version.as_str());
        let patch_version = flash.as_ref().map_or("?", |f| f.patch_version.as_str());
        let hardware = flash.as_ref().map_or("Unknown", |f| f.hw_rev.as_str());
        let rom_data_ptr = ram.as_ref().map_or(0, |f| f.rom_data_ptr);
        let rtt_ptr = flash.as_ref().map_or(0, |f| f.rtt_ptr);

        let html = format!(r#"
<tr>
<td class="label-col"><strong>Firmware:</strong></td>
<td>One ROM Lab</td>
</tr>
<tr>
<td class="label-col"><strong>Version:</strong></td>
<td>V{}.{}.{}</td>
</tr>
<tr>
<td class="label-col"><strong>Hardware:</strong></td>
<td>{}</td>
</tr>
<tr>
<td class="label-col"><strong>ROM data:</strong></td>
<td>{:#010X}</td>
</tr>
<tr>
<td class="label-col"><strong>RTT data:</strong></td>
<td>{:#010X}</td>
</tr>
        "#,
        major_version,
        minor_version,
        patch_version,
        hardware,
        rom_data_ptr,
        rtt_ptr,
    );

        Ok(html)
    }

    fn complete(&self, data: serde_json::Value) -> Result<String, FormatterError> {
                let html = format!(r#"
<div class="card">
<h2>One ROM Lab</h2>
<table class="device-info">
{}
</table>
</div>
        "#,
            self.summary(data)?
            );

        Ok(html)
    }
}