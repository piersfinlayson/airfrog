// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - One ROM Firmware handling

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use sdrr_fw_parser::{Parser, Reader as SdrrReader, Sdrr, SdrrInfo, SdrrRuntimeInfo, SdrrServe};

use crate::firmware::AF_FW_TYPE_KEY;
use crate::firmware::{FormatterError, FwError, FwHandler, FwHandlerInfo, FwInfo, JsonToHtml};
use airfrog_core::Mcu;

pub(crate) const AF_FW_TYPE: &str = "SdrrInfo";

/// SDRR firmware handler information
pub struct SdrrHandlerInfo;

impl FwHandlerInfo for SdrrHandlerInfo {
    /// Returns the name/identifier of this firmware type
    fn name() -> &'static str {
        "One ROM"
    }

    /// Returns whether this handler supports the given MCU
    fn supports_mcu(mcu: &Mcu) -> bool {
        mcu.is_stm32f4()
    }
}

/// The SDRR firmware handler
pub struct SdrrHandler<R: SdrrReader> {
    parser: Parser<R>,
}

impl<R: SdrrReader> FwHandler<R> for SdrrHandler<R> {
    fn new(reader: R) -> Self {
        let parser = Parser::new(reader);
        Self { parser }
    }

    async fn detect(&mut self) -> bool {
        self.parser.detect().await
    }

    /// Attempts to detect if this firmware type is present on the target
    /// Returns true if detected, false otherwise
    async fn parse_info(&mut self) -> Result<Box<dyn FwInfo>, FwError<R::Error>> {
        let sdrr = self.parser.parse().await;
        Ok(Box::new(FwSdrr(sdrr)))
    }
}

pub struct FwSdrr(Sdrr);

impl FwInfo for FwSdrr {
    fn summary(&self) -> serde_json::Value {
        let content = &self.0;
        match content.flash.as_ref() {
            Some(flash) => {
                serde_json::json!({
                    "version": format!("v{}.{}.{}", flash.major_version, flash.minor_version, flash.patch_version),
                    "build_date": flash.build_date,
                    "hw_rev": flash.hw_rev,
                })
            }
            None => {
                serde_json::json!({
                    "error": "No flash content found in SDRR firmware"
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
    pub fn get_sdrr(&self, data: serde_json::Value) -> Result<Sdrr, FormatterError> {
        if !self.can_handle(&data) {
            return Err(FormatterError::JsonToHtml(
                "Unsupported firmware data".to_string(),
            ));
        }

        match serde_json::from_value(data) {
            Ok(info) => Ok(info),
            Err(e) => Err(FormatterError::JsonToHtml(format!(
                "Failed to parse SDRR info: {e}"
            ))),
        }
    }

    pub fn _get_flash_info(&self, data: serde_json::Value) -> Result<SdrrInfo, FormatterError> {
        let sdrr = self.get_sdrr(data)?;
        if let Some(flash_info) = sdrr.flash {
            Ok(flash_info)
        } else {
            Err(FormatterError::JsonToHtml(
                "SDRR firmware information incomplete".to_string(),
            ))
        }
    }

    pub fn _get_ram_info(
        &self,
        data: serde_json::Value,
    ) -> Result<SdrrRuntimeInfo, FormatterError> {
        let sdrr = self.get_sdrr(data)?;
        if let Some(ram_info) = sdrr.ram {
            Ok(ram_info)
        } else {
            Err(FormatterError::JsonToHtml(
                "SDRR runtime information incomplete".to_string(),
            ))
        }
    }

    fn push_table_row(&self, item: &str, output: &str, html: &mut String) {
        html.push_str(&format!(
            r#"<tr>
<td class="label-col"><strong>{item}:</strong></td>
<td>{output}</td>
</tr>"#,
        ));
    }

    fn get_current_rom_info(&self, fw_info: &SdrrInfo, ram_info: &SdrrRuntimeInfo) -> String {
        let set_index = ram_info.rom_set_index as usize;

        if set_index >= fw_info.rom_sets.len() {
            return format!("{set_index} - unknown image");
        }

        let rom_set = &fw_info.rom_sets[set_index];
        let names: Vec<String> = rom_set
            .roms
            .iter()
            .map(|rom| rom.filename.as_deref().unwrap_or("<unnamed>"))
            .map(|name| name.to_string())
            .collect();

        if names.is_empty() {
            format!("{set_index} - unknown image")
        } else {
            format!("{set_index} - <strong>{}</strong>", names.join("/"))
        }
    }
}

#[allow(clippy::collapsible_if)]
impl JsonToHtml for JsonToHtmler {
    fn can_handle(&self, data: &serde_json::Value) -> bool {
        data.get("_af_fw_type").is_some_and(|t| t == "SdrrInfo")
    }

    fn summary(&self, data: serde_json::Value) -> Result<String, FormatterError> {
        let sdrr = self.get_sdrr(data)?;
        let fw_info = sdrr.flash;
        let ram_info = sdrr.ram;

        let mut html = String::with_capacity(1024);

        let item = "Firmware";
        let output = "One ROM";
        self.push_table_row(item, output, &mut html);

        let item = "Version";
        let output = if let Some(fw_info) = &fw_info {
            format!(
                "V{}.{}.{}</td>",
                fw_info.major_version, fw_info.minor_version, fw_info.patch_version
            )
        } else {
            "unknown".to_string()
        };
        self.push_table_row(item, &output, &mut html);

        let item = "Serving ROM/set";
        let output = if let Some(ram_info) = &ram_info {
            if let Some(fw_info) = &fw_info {
                self.get_current_rom_info(fw_info, ram_info).to_string()
            } else {
                format!("{} - unknown image", ram_info.rom_set_index)
            }
        } else {
            "unknown".to_string()
        };
        self.push_table_row(item, &output, &mut html);

        let item = "Bytes served";
        let output = if let Some(ram_info) = &ram_info {
            if ram_info.count_rom_access > 0 {
                format!("{}", ram_info.last_parsed_access_count)
            } else {
                "unavailable".to_string()
            }
        } else {
            "unknown".to_string()
        };
        self.push_table_row(item, &output, &mut html);

        let item = "Emulating";
        let output = if let Some(fw_info) = &fw_info
            && let Some(pins) = &fw_info.pins
        {
            format!("{} pin ROM", pins.rom_pins)
        } else {
            "unknown ROM".to_string()
        };
        self.push_table_row(item, &output, &mut html);

        let item = "Hardware Revision";
        let output = if let Some(fw_info) = &fw_info
            && let Some(hw_rev) = &fw_info.hw_rev
        {
            hw_rev.clone()
        } else {
            "unknown".to_string()
        };
        self.push_table_row(item, &output, &mut html);

        Ok(html)
    }

    fn complete(&self, data: serde_json::Value) -> Result<String, FormatterError> {
        let sdrr = self.get_sdrr(data)?;
        let fw_info = sdrr.flash;
        let ram_info = sdrr.ram;

        let mut html = String::with_capacity(8192);

        // Core Firmware Properties
        html.push_str("<div class=\"card\">");
        html.push_str("<h2>One ROM</h2>");

        html.push_str("<h3>Core Firmware Properties</h3>");

        if let Some(fw_info) = &fw_info {
            html.push_str(&format!(
                "<p><strong>Version:</strong> {}.{}.{} (build {})</p>",
                fw_info.major_version,
                fw_info.minor_version,
                fw_info.patch_version,
                fw_info.build_number
            ));

            if let Some(build_date) = &fw_info.build_date {
                html.push_str(&format!("<p><strong>Build Date:</strong> {build_date}</p>"));
            }

            let commit_str: String = fw_info
                .commit
                .iter()
                .take_while(|&&b| b != 0)
                .map(|&b| b as char)
                .collect();
            html.push_str(&format!("<p><strong>Git commit:</strong> {commit_str}</p>"));

            if let Some(hw_rev) = &fw_info.hw_rev {
                html.push_str(&format!("<p><strong>Hardware:</strong> {hw_rev}</p>"));
            }

            html.push_str(&format!(
                "<p><strong>STM32:</strong> {} / {}</p>",
                fw_info.stm_line, fw_info.stm_storage
            ));

            html.push_str(&format!(
                "<p><strong>Frequency:</strong> {} MHz (Overclocking: {})</p>",
                fw_info.freq, fw_info.overclock
            ));
        } else {
            html.push_str("<p><strong>Error:</strong> No information available from flash</p>");
        }

        html.push_str("<h3>Runtime Properties</h3>");

        if let Some(ram_info) = &ram_info {
            let output = if let Some(fw_info) = &fw_info {
                self.get_current_rom_info(fw_info, ram_info).to_string()
            } else {
                format!("{} - unknown image", ram_info.rom_set_index)
            };

            html.push_str(&format!(
                "<p><strong>Serving ROM/set:</strong> {output}</p>",
            ));

            html.push_str(&format!(
                "<p><strong>Image select jumpers:</strong> {}</p>",
                ram_info.image_sel
            ));

            if ram_info.count_rom_access > 0 {
                html.push_str(&format!(
                    "<p><strong>Bytes served:</strong> {}</p>",
                    ram_info.last_parsed_access_count
                ));
            } else {
                html.push_str("<p><strong>Bytes served:</strong> access count disabled</p>");
            }
            html.push_str(&format!(
                "<p><strong>Access count address:</strong> {:#010X}</p>",
                ram_info.account_count_address
            ));
            html.push_str(&format!(
                "<p><strong>ROM table address:</strong> {:#010X}</p>",
                ram_info.rom_table_address
            ));
            html.push_str(&format!(
                "<p><strong>ROM table size:</strong> {} bytes</p>",
                ram_info.rom_table_size
            ));
            if let Some(fw_info) = &fw_info
                && fw_info.boot_logging_enabled
                && let Some(extra_info) = &fw_info.extra_info
            {
                html.push_str(&format!(
                    "<p><strong>RTT Control Block:</strong> {:#010X}</p>",
                    extra_info.rtt_ptr
                ));
            }
        } else {
            html.push_str("<p><strong>Error:</strong> No information available from RAM</p>");
        }

        html.push_str("</div>");

        if let Some(fw_info) = &fw_info {
            // Configurable Options
            html.push_str("<div class=\"card\">");
            html.push_str("<h2>Configurable Options</h2>");

            if let Some(pins) = &fw_info.pins {
                html.push_str(&format!(
                    "<p><strong>ROM emulation:</strong> {} pin ROM</p>",
                    pins.rom_pins
                ));
            }

            let preload = if fw_info.preload_image_to_ram {
                "RAM"
            } else {
                "false"
            };
            html.push_str(&format!(
                "<p><strong>Serve image from:</strong> {preload}</p>"
            ));

            html.push_str(&format!(
                "<p><strong>SWD enabled:</strong> {}</p>",
                fw_info.swd_enabled
            ));

            html.push_str(&format!(
                "<p><strong>Boot logging:</strong> {}</p>",
                fw_info.boot_logging_enabled
            ));

            html.push_str(&format!(
                "<p><strong>Status LED:</strong> {}</p>",
                fw_info.status_led_enabled
            ));

            let bootloader = if fw_info.bootloader_capable {
                "true"
            } else {
                "false"
            };
            html.push_str(&format!(
                "<p><strong>STM bootloader:</strong> {bootloader}</p>"
            ));

            let mco = if fw_info.mco_enabled {
                "true (exposed via test pad)"
            } else {
                "false"
            };
            html.push_str(&format!("<p><strong>MCO enabled:</strong> {mco}</p>"));

            let boot_config = &fw_info.boot_config;
            html.push_str(&format!(
                "<p><strong>Boot config:</strong> 0x{:02X}{:02X}{:02X}{:02X}</p>",
                boot_config[0], boot_config[1], boot_config[2], boot_config[3]
            ));

            html.push_str("</div>");

            // ROMs Summary
            html.push_str("<h2>ROMs Summary</h2>");

            html.push_str("<div class=\"card\">");

            html.push_str(&format!(
                "<p><strong>Total sets:</strong> {}</p>",
                fw_info.rom_set_count
            ));

            let total_roms: usize = fw_info.rom_sets.iter().map(|set| set.roms.len()).sum();
            html.push_str(&format!("<p><strong>Total ROMs:</strong> {total_roms}</p>"));

            html.push_str("</div>");

            // ROM Details
            html.push_str("<h2>ROM Details</h2>");
            for (i, rom_set) in fw_info.rom_sets.iter().enumerate() {
                if i > 0 {
                    html.push_str("<hr>");
                }

                html.push_str("<div class=\"card\">");

                html.push_str(&format!("<h3>ROM Set: {i}</h3>"));

                let rom_count = rom_set.rom_count;
                let set_type = if matches!(rom_set.serve, SdrrServe::AddrOnAnyCs) {
                    "Multi-ROM socket"
                } else if rom_count > 1 {
                    "Dynamic bank switching"
                } else {
                    "Single ROM image"
                };

                html.push_str(&format!("<p><strong>Set type:</strong> {set_type}</p>"));
                html.push_str(&format!(
                    "<p><strong>Size:</strong> {} bytes</p>",
                    rom_set.size
                ));
                html.push_str(&format!("<p><strong>ROM Count:</strong> {rom_count}</p>"));
                html.push_str(&format!(
                    "<p><strong>Algorithm:</strong> {}</p>",
                    rom_set.serve
                ));
                html.push_str(&format!(
                    "<p><strong>Multi-ROM CS1:</strong> {}</p>",
                    rom_set.multi_rom_cs1_state
                ));

                html.push_str("</div>");

                for (j, rom) in rom_set.roms.iter().enumerate() {
                    html.push_str("<div class=\"card\">");

                    html.push_str(&format!("<h4>ROM: {j}</h4>"));

                    let filename = rom.filename.as_deref().unwrap_or("<not present>");
                    html.push_str(&format!("<p><strong>Name:</strong> {filename}</p>"));

                    html.push_str(&format!("<p><strong>Type:</strong> {}</p>", rom.rom_type));

                    html.push_str(&format!(
                        "<p><strong>CS States:</strong> {}/{}/{}</p>",
                        rom.cs1_state, rom.cs2_state, rom.cs3_state
                    ));
                    html.push_str("</div>");
                }
            }

            // Pin Configuration
            if let Some(pins) = &fw_info.pins {
                html.push_str("<h2>Pin Configuration</h2>");

                // Data pin mapping
                html.push_str("<div class=\"card\">");
                html.push_str("<h3>Data pin mapping:</h3>");
                for (i, &pin_num) in pins.data.iter().enumerate() {
                    if pin_num != 255 {
                        html.push_str(&format!("<p>D{i}: P{}:{pin_num}</p>", pins.data_port));
                    }
                }
                html.push_str("</div>");

                // Address pin mapping
                html.push_str("<div class=\"card\">");
                html.push_str("<h3>Address pin mapping:</h3>");
                for (i, &pin_num) in pins.addr.iter().enumerate() {
                    if pin_num != 255 {
                        html.push_str(&format!("<p>A{i}: P{}:{pin_num}</p>", pins.addr_port));
                    }
                }
                html.push_str("</div>");

                // Chip select pins
                html.push_str("<div class=\"card\">");
                html.push_str("<h3>Chip select pins:</h3>");
                let cs_pins = [
                    (pins.cs1_2364, "2364 CS1"),
                    (pins.cs1_2332, "2332 CS1"),
                    (pins.cs2_2332, "2332 CS2"),
                    (pins.cs1_2316, "2316 CS1"),
                    (pins.cs2_2316, "2316 CS2"),
                    (pins.cs3_2316, "2316 CS3"),
                    (pins.ce_23128, "23128 CE"),
                    (pins.oe_23128, "23128 OE"),
                    (pins.x1, "Multi X1"),
                    (pins.x2, "Multi X2"),
                ];

                for (pin_num, label) in &cs_pins {
                    if *pin_num != 255 {
                        html.push_str(&format!("<p>{label}: P{}:{pin_num}</p>", pins.cs_port));
                    }
                }
                if pins.x1 != 255 || pins.x2 != 255 {
                    html.push_str(&format!(
                        "<p>Multi X1/X2 jumper pull: {}</p>",
                        pins.x_jumper_pull
                    ));
                }
                html.push_str(&format!("<p>X1/2 jumper pull: {}</p>", pins.x_jumper_pull));
                html.push_str("</div>");

                // Image select pins
                html.push_str("<div class=\"card\">");
                html.push_str("<h3>Image select pins:</h3>");
                let sel_pins = [
                    pins.sel0, pins.sel1, pins.sel2, pins.sel3, pins.sel4, pins.sel5, pins.sel6,
                ];
                for (i, pin_num) in sel_pins.iter().enumerate() {
                    if *pin_num != 255 {
                        html.push_str(&format!("<p>SEL{i}: P{}:{pin_num}</p>", pins.sel_port));
                    }
                }
                html.push_str(&format!(
                    "<p>Select jumper pull: {}</p>",
                    pins.sel_jumper_pull
                ));
                html.push_str("</div>");

                // Status LED pin
                html.push_str("<div class=\"card\">");
                html.push_str("<h3>Status LED pin:</h3>");
                if pins.status_port.to_string() == "None" {
                    html.push_str("<p>Pin: None</p>");
                } else if pins.status != 255 {
                    html.push_str(&format!(
                        "<p>Pin: P{}:{}</p>",
                        pins.status_port, pins.status
                    ));
                }
                html.push_str("</div>");
            }

            // Parse errors if any
            if !fw_info.parse_errors.is_empty() {
                html.push_str("<h2>Parse Errors</h2>");
                html.push_str("<ul>");
                for error in &fw_info.parse_errors {
                    html.push_str(&format!("<li>{error}</li>"));
                }
                html.push_str("</ul>");
            }
        }

        Ok(html)
    }
}
