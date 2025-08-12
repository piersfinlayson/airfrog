// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - HTML strings and objects

use alloc::format;
use alloc::string::{String, ToString};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use strum::IntoEnumIterator;

use crate::AirfrogError;
use crate::config::{CONFIG, NetMode};
use crate::device::Device;
use crate::firmware::{JsonToHtml, JsonToHtmlers};
use crate::http::assets::{
    BROWSER_HTML_PATH, CONFIG_UPDATE_JS_PATH, CSS_PATH, FAVICON_PATH, FOOTER_HTML_PATH, LOGO_PATH,
    MEMORY_CSS_PATH, MEMORY_JS_PATH,
};
use crate::target::{Response, Settings, Status};
use crate::{
    AIRFROG_BUILD_DATE, AIRFROG_BUILD_TIME, AIRFROG_HOME_PAGE, AUTHOR, AUTHOR_EMAIL,
    FEATURES_LOWERCASE_STR, PKG_LICENSE, PKG_VERSION, PROFILE, RUSTC_VERSION,
};

/// An HTML content object used to return HTML responses to picoserve
pub struct HtmlContent(pub String);

impl HtmlContent {
    fn header() -> String {
        format!(
            r#"<!DOCTYPE html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><link rel='icon' type='image/png' sizes='32x32' href='{FAVICON_PATH}'><link rel='stylesheet' href='{CSS_PATH}'><title>airfrog</title></head>"#
        )
    }

    fn logo() -> String {
        format!(r#"<img src='{LOGO_PATH}' alt='logo' class='logo'>"#)
    }

    fn footer() -> String {
        format!(
            r#"<div id="ft-phdr"></div><script>fetch('{FOOTER_HTML_PATH}').then(r => r.text()).then(html => document.getElementById('ft-phdr').innerHTML = html);</script>"#
        )
    }

    fn body(body: &str) -> String {
        let logo = Self::logo();
        let footer = Self::footer();
        format!(r#"<body>{logo}<div class="content">{body}</div>{footer}</body>"#,)
    }

    pub(crate) fn new(body: String) -> Self {
        let full_html = format!("{}{}", Self::header(), Self::body(&body),);
        HtmlContent(full_html)
    }
}

impl From<AirfrogError> for HtmlContent {
    fn from(e: AirfrogError) -> Self {
        HtmlContent::new(format!("<h1>Error</h1><p>{e}</p>"))
    }
}
pub(crate) fn html_summary(
    status: Option<Status>,
    firmware: Option<serde_json::Value>,
    output_fw_summary: bool,
) -> String {
    let connected = status.as_ref().is_some_and(|s| s.connected);

    let (status_class, status_text) = if connected {
        ("status ok", "Connected")
    } else {
        ("status error", "Disconnected")
    };

    let auto = if let Some(status) = status.as_ref() {
        if status.settings.auto_connect {
            "Enabled"
        } else {
            "Disabled"
        }
    } else {
        "Unknown"
    };
    let auto = format!(
        "<tr><td class=\"label-col\"><strong>Auto Connect:</strong></td><td>{auto}</td></tr>",
    );

    let mcu = status
        .as_ref()
        .and_then(|s| s.mcu.as_deref())
        .unwrap_or("None");

    let (mcu, fw_rows) = if output_fw_summary {
        if connected {
            let firmware_html = if let Some(fw) = firmware {
                html_firmware_summary(fw)
            } else {
                None
            };
            let fw_rows = match firmware_html {
                Some(html) => html,
                None => format!(
                    "<tr><td class=\"label-col\"><strong>Firmware:</strong></td><td>Unknown</td></tr>{auto}"
                ),
            };
            (mcu, fw_rows)
        } else {
            (
                "None",
                format!(
                    "<tr><td class=\"label-col\"><strong>Firmware:</strong></td><td>None</td></tr>{auto}"
                ),
            )
        }
    } else if connected {
        (
            mcu,
            "<tr><td class=\"label-col\"><strong>Firmware:</strong></td><td>Details below</td></tr>".to_string()
        )
    } else {
        (
            mcu,
            format!(
                "<tr><td class=\"label-col\"><strong>Firmware:</strong></td><td>None</td></tr>{auto}"
            ),
        )
    };

    format!(
        r#"
<div class="card">
  <h2 style="text-align: center; transform: translateX(-100px)">Target Status 
    <span class="{status_class}">{status_text}</span>
    <button class="refresh-btn inline" onclick="location.reload()">Refresh</button>
  </h3>
    <table class="device-info">
    <tr><td class="label-col"><strong>MCU:</strong></td><td>{mcu}</td></tr>
    {fw_rows}
    </table>
</div>"#
    )
}

/// Generate the HTML for the dashboard page
///
/// This is fairly small so we generate it dynamically rather than store it in
/// a static file.
pub(crate) fn page_dashboard(summary: String) -> HtmlContent {
    let body = format!(
        r#"
<h1>Airfrog Dashboard</h1>
{summary}
"#
    );
    HtmlContent::new(body)
}

/// Generate the HTML for the browser page
///
/// Most of the content is stored in a static HTML file - we need some JS to
/// load that HTML into the page.  This reduces the amount of HTML that gets
/// loaded into RAM.
pub(crate) fn page_target_browser(summary: String) -> HtmlContent {
    let body = format!(
        r#"
<h1>Target Memory Access</h1>
{summary}
<link rel="stylesheet" href="{MEMORY_CSS_PATH}">
<div id="loading-card" class="card">
  Loading MCU Browser...
</div>
<div id="content-placeholder" style="display: none;"></div>
<script>
fetch('{BROWSER_HTML_PATH}')
  .then(r => r.text())
  .then(html => {{
    document.getElementById('content-placeholder').innerHTML = html;
    document.getElementById('loading-card').style.display = 'none';
    document.getElementById('content-placeholder').style.display = 'block';
  }});
</script>
<script src="{MEMORY_JS_PATH}"></script>"#
    );

    HtmlContent::new(body)
}

// Used by html_firmware_complete to generate the firmware summary
fn html_firmware_summary(json: serde_json::Value) -> Option<String> {
    let mut result = None;
    for handler in JsonToHtmlers::iter() {
        if handler.can_handle(&json) {
            match handler.summary(json) {
                Ok(fw) => {
                    result = Some(fw);
                    break;
                }
                Err(e) => {
                    warn!("Failed to convert JSON to HTML: {e:?}");
                    return None;
                }
            }
        }
    }
    result
}

pub(crate) fn page_target_firmware(response: Response) -> HtmlContent {
    let json = response.data;
    let fw_info_html = if let Some(json) = json {
        let mut result = None;
        for handler in JsonToHtmlers::iter() {
            if handler.can_handle(&json) {
                match handler.complete(json) {
                    Ok(fw) => {
                        result = Some(fw);
                        break;
                    }
                    Err(e) => {
                        warn!("Failed to convert JSON to HTML: {e:?}");
                        result = Some("<div id=\"fw-info\" class=\"card\"><p>Unable to parse firmware information.</p></div>".to_string());
                        break;
                    }
                }
            }
        }
        result.unwrap_or(
            "<div id=\"fw-info\" class=\"card\"><p>Unable to parse firmware type.</p></div>"
                .to_string(),
        )
    } else {
        "<div id=\"fw-info\" class=\"card\"><p>No firmware information available.</p><br/><br/><br/><br/><br/></div>"
            .to_string()
    };

    let body = format!(
        r#"
<h1>Target Firmware Information</h1>
{fw_info_html}"#
    );

    HtmlContent::new(body)
}

pub(crate) fn page_target_update(summary: String) -> HtmlContent {
    let body = format!(
        r#"
<h1>Target Flash Operations</h1>
{summary}
<link rel="stylesheet" href="{MEMORY_CSS_PATH}">
<div class="card">
  <p><strong>⚠️ Warning: These operations can brick the target, requiring reflashing.</strong></p>
  <br/>
  <div style="display: flex; justify-content: space-between; align-items: flex-start; margin: 1rem 0;">
    <table class="device-info" style="margin: 0;">
      <tr>
        <td class="label-col" style="width: 300px;"><strong>Erase before update:</strong></td>
        <td><input type="checkbox" id="eraseBeforeUpdate" title="Erase the flash memory before updating" checked></td>
      </tr>
      <tr>
        <td class="label-col" style="width: 300px;"><strong>Halt before operation:</strong></td>
        <td><input type="checkbox" id="resetBefore" title="Halt the target before performing the erase or flash update" checked></td>
      </tr>
      <tr>
        <td class="label-col" style="width: 300px;"><strong>Reset after operation:</strong></td>
        <td><input type="checkbox" id="resetAfter" title="Reset the target after performing the erase or flash update" checked></td>
      </tr>
      <tr>
        <td class="label-col" style="width: 300px;"><strong>Verify after update:</strong></td>
        <td><input type="checkbox" id="verifyAfterUpdate" title="Verify the flash after updating" checked></td>
      </tr>
    </table>
  <div style="display: flex; flex-direction: column; gap: 5px; min-width: 150px;">
    <button title="Halt the target MCU immediately" id="haltButton">Halt Target</button>
    <button title="Reset and restart the target MCU immediately" id="resetButton">Reset Target</button>
    <button title="Erase the target's flash" id="eraseButton">Erase Flash</button>
  </div>
</div>
  <div class="upload-section">
    <input type="file" id="firmwareFile" accept=".bin,.hex,.elf" style="display: none;" />
    <button type="button" id="chooseFileBtn">Choose File</button>
    <strong id="fileStatus">No file chosen</strong>
    <button id="updateButton" title="Update the target's flash with the selected firmware file" disabled style="float: right">Update Firmware</button>
  </div>
  <div class="progress-section" style="display: none;">
    <div class="progress-bar">
      <div class="progress-fill"></div>
    </div>
    <div class="progress-text">0%</div>
  </div>
  <div id="statusMessage" class="status-message"></div>
</div>
<script src="{MEMORY_JS_PATH}"></script>"#
    );

    HtmlContent::new(body)
}

// Airfrog runtime information
fn html_system_info(flash_size_bytes: usize) -> String {
    // Get heap stats
    let heap_size = Device::heap_size();
    let heap_used = Device::heap_used();
    let heap_pct = (heap_used * 100) / heap_size;

    // Get uptime
    let uptime_secs = Device::uptime_secs();
    let ut_days = uptime_secs / 86400;
    let ut_hours = (uptime_secs % 86400) / 3600;
    let ut_minutes = (uptime_secs % 3600) / 60;
    let ut_seconds = uptime_secs % 60;

    // Get clock speed
    let clock_speed = Device::clock_speed_mhz();

    // Get the chip ID
    let mcu = Device::chip();

    // Last reset reason
    let rr_str = Device::reset_reason();

    // Get mac address
    let mac_address = Device::mac_address_str();

    // Turn flash size into KB
    let flash_size_kb = flash_size_bytes / 1024;

    format!(
        r#"
<div class="card">
<h2>System</h2>
  <table class="device-info">
    <tr><td class="label-col" style="width: 300px;"><strong>MCU:</strong></td><td>{mcu}</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Flash size:</strong></td><td>{flash_size_kb} KB</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Clock Speed:</strong></td><td>{clock_speed} MHz</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>MAC Address:</strong></td><td>{mac_address}</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Last Reset Reason:</strong></td><td>{rr_str}</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Uptime:</strong></td><td>{ut_days}d {ut_hours}h {ut_minutes}m {ut_seconds}s</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Heap:</strong></td><td>{heap_used}/{heap_size} bytes ({heap_pct:.1}% used)</td></tr>
  </table>
</div>"#,
    )
}

// Airfrog build level information
fn html_build_info() -> String {
    // Rust version
    let rust = RUSTC_VERSION
        .strip_prefix("rustc ")
        .unwrap_or(RUSTC_VERSION);

    format!(
        r#"
<div class="card">
  <h2>Build</h2>
  <table class="device-info">
    <tr><td class="label-col" style="width: 300px;"><strong>Airfrog Version:</strong></td><td>v{PKG_VERSION}</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Build Time and Date:</strong></td><td>{AIRFROG_BUILD_TIME} {AIRFROG_BUILD_DATE}</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Build Features:</strong></td><td>{FEATURES_LOWERCASE_STR}</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Build Profile:</strong></td><td>{PROFILE}</td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Rust Version:</strong></td><td>{rust}</td></tr>
  </table>
</div>"#
    )
}

// Airfrog project level information
fn html_project_info() -> String {
    format!(
        r#"
<div class="card">
  <h2>Project</h2>
  <table class="device-info">
    <tr><td class="label-col" style="width: 300px;"><strong>Homepage:</strong></td><td><a href="https://{AIRFROG_HOME_PAGE}">{AIRFROG_HOME_PAGE}</a></td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Author:</strong></td><td><a href="mailto:{AUTHOR_EMAIL}?subject=Airfrog">{AUTHOR}</a></td></tr>
    <tr><td class="label-col" style="width: 300px;"><strong>Licence:</strong></td><td>{PKG_LICENSE}</td></tr>
  </table>
</div>"#
    )
}

/// Generate Airfrog Information page.
pub(crate) fn page_info(flash_size_bytes: usize) -> HtmlContent {
    let system = html_system_info(flash_size_bytes);
    let build = html_build_info();
    let project = html_project_info();

    let body = format!(
        r#"
<h1>Airfrog Information</h1>
{system}
{build}
{project}
"#
    );

    HtmlContent::new(body)
}

// Used for both runtime and stored SWD settings
pub(crate) fn settings_swd(prefix: &str, source: &str, settings: &Settings) -> String {
    let slow_selected = if format!("{:?}", settings.speed) == "Slow" {
        "selected"
    } else {
        ""
    };
    let medium_selected = if format!("{:?}", settings.speed) == "Medium" {
        "selected"
    } else {
        ""
    };
    let fast_selected = if format!("{:?}", settings.speed) == "Fast" {
        "selected"
    } else {
        ""
    };
    let turbo_selected = if format!("{:?}", settings.speed) == "Turbo" {
        "selected"
    } else {
        ""
    };
    let auto_connect_checked = if settings.auto_connect { "checked" } else { "" };
    let keepalive_checked = if settings.keepalive { "checked" } else { "" };
    let refresh_checked = if settings.refresh { "checked" } else { "" };

    format!(
        r#"
  <table class="device-info">
    <tr>
      <td class="label-col"><strong>SWD speed:</strong></td>
      <td>
        <select id="{prefix}Sp" title="Controls how fast Airfrog communicates with the target - use slower speeds where wiring is long or unreliable">
          <option value="Slow" {slow_selected}>Slow (500kHz)</option>
          <option value="Medium" {medium_selected}>Medium (1MHz)</option>
          <option value="Fast" {fast_selected}>Fast (2MHz)</option>
          <option value="Turbo" {turbo_selected}>Turbo (4MHz)</option>
        </select>
      </td>
    </tr>
    <tr>
      <td class="label-col" style="width: 300px;"><strong>Auto connect to target:</strong></td>
      <td><input type="checkbox" id="{prefix}Ac" title="Periodically attempts to reconnects to the target when disconnected" {auto_connect_checked}></td>
    </tr>
    <tr>
      <td class="label-col" style="width: 300px;"><strong>Target keepalive:</strong></td>
      <td><input type="checkbox" id="{prefix}Ka" title="Periodically polls the target when connected" {keepalive_checked}></td>
    </tr>
    <tr>
      <td class="label-col" style="width: 300px;"><strong>Firmware refresh:</strong></td>
      <td><input type="checkbox" id="{prefix}Rst" title="Periodically re-analyses the target firmware when connected" {refresh_checked}></td>
    </tr>
  </table>
  <div style="text-align: center; margin-top: 1rem;">
    <button id="{prefix}Up">Update {source} Settings</button>
  </div>
  <div id="{prefix}Stat" class="status-message"></div>
"#
    )
}

pub(crate) fn settings_swd_runtime(settings: &Settings) -> String {
    settings_swd("sr", "Runtime", settings)
}

pub(crate) async fn settings_swd_stored() -> String {
    let config = CONFIG.get().await.lock().await;
    let swd = &config.swd;
    let settings = Settings {
        speed: swd.speed.into(),
        auto_connect: swd.auto_connect == 1,
        keepalive: swd.keep_alive == 1,
        refresh: swd.refresh == 1,
    };
    settings_swd("st", "Stored", &settings)
}

pub(crate) async fn settings_net_stored() -> String {
    let config = CONFIG.get().await.lock().await;
    let net = &config.net;

    // Convert byte arrays to strings
    let sta_ssid = net.sta_ssid().unwrap_or_default();
    let sta_password = net.sta_password().unwrap_or_default();

    let ap_ssid = net.ap_ssid().unwrap_or_default();
    let ap_password = net.ap_password().unwrap_or_default();

    // Network mode selection
    let mode_sta_selected = if net.mode() == NetMode::StaFallbackToAp {
        "selected"
    } else {
        ""
    };
    let mode_ap_selected = if net.mode() == NetMode::ApOnly {
        "selected"
    } else {
        ""
    };

    // DHCP checkbox
    let dhcp_checked = if net.sta_v4_dhcp() { "checked" } else { "" };

    // IP address formatting
    let format_ip = |ip: &[u8; 4]| -> String {
        if ip == &[0xFF; 4] {
            String::new()
        } else {
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        }
    };

    let sta_ip = format_ip(&net.sta_v4_ip());
    let sta_netmask = format_ip(&net.sta_v4_netmask());
    let sta_gw = format_ip(&net.sta_v4_gw());
    let sta_dns0 = format_ip(&net.sta_v4_dns0());
    let sta_dns1 = format_ip(&net.sta_v4_dns1());
    let sta_ntp = format_ip(&net.sta_v4_ntp());
    let ap_ip = format_ip(&net.ap_v4_ip());
    let ap_netmask = format_ip(&net.ap_v4_netmask());

    format!(
        r#"
  <table class="device-info">
    <tr>
      <td class="label-col"><strong>Network Mode:</strong></td>
      <td>
        <select id="ntMode" title="Network operation mode">
          <option value="StaFallbackToAp" {mode_sta_selected}>Station (fallback to AP)</option>
          <option value="ApOnly" {mode_ap_selected}>Access Point only</option>
        </select>
      </td>
    </tr>
    <tr><td colspan="2"><h3>Station Settings</h3></td></tr>
    <tr>
      <td class="label-col"><strong>SSID:</strong></td>
      <td><input type="text" id="ntStaSsid" value="{sta_ssid}" maxlength="31" title="Network name to connect to"></td>
    </tr>
    <tr>
      <td class="label-col"><strong>Password:</strong></td>
      <td><input type="password" id="ntStaPass" value="{sta_password}" maxlength="63" title="Network password"></td>
    </tr>
    <tr>
      <td class="label-col"><strong>Use DHCP:</strong></td>
      <td><input type="checkbox" id="ntStaDhcp" {dhcp_checked} title="Get IP address automatically"></td>
    </tr>
    <tr id="ntStaIpRow">
      <td class="label-col"><strong>IP Address:</strong></td>
      <td><input type="text" id="ntStaIp" value="{sta_ip}" placeholder="192.168.1.100" title="Static IP address"></td>
    </tr>
    <tr id="ntStaNetmaskRow">
      <td class="label-col"><strong>Netmask:</strong></td>
      <td><input type="text" id="ntStaNetmask" value="{sta_netmask}" placeholder="255.255.255.0" title="Subnet mask"></td>
    </tr>
    <tr id="ntStaGwRow">
      <td class="label-col"><strong>Gateway:</strong></td>
      <td><input type="text" id="ntStaGw" value="{sta_gw}" placeholder="192.168.1.1" title="Default gateway"></td>
    </tr>
    <tr id="ntStaDns0Row">
      <td class="label-col"><strong>Primary DNS:</strong></td>
      <td><input type="text" id="ntStaDns0" value="{sta_dns0}" placeholder="8.8.8.8" title="Primary DNS server"></td>
    </tr>
    <tr id="ntStaDns1Row">
      <td class="label-col"><strong>Secondary DNS:</strong></td>
      <td><input type="text" id="ntStaDns1" value="{sta_dns1}" placeholder="8.8.4.4" title="Secondary DNS server"></td>
    </tr>
    <tr id="ntStaNtpRow">
      <td class="label-col"><strong>NTP Server:</strong></td>
      <td><input type="text" id="ntStaNtp" value="{sta_ntp}" placeholder="192.168.1.100" title="NTP server"></td>
    </tr>
    <tr><td colspan="2"><h3>Access Point Settings</h3></td></tr>
    <tr>
      <td class="label-col"><strong>SSID:</strong></td>
      <td><input type="text" id="ntApSsid" value="{ap_ssid}" maxlength="31" title="Access point network name"></td>
    </tr>
    <tr>
      <td class="label-col"><strong>Password:</strong></td>
      <td><input type="password" id="ntApPass" value="{ap_password}" maxlength="63" title="Access point password"></td>
    </tr>
    <tr>
      <td class="label-col"><strong>IP Address:</strong></td>
      <td><input type="text" id="ntApIp" value="{ap_ip}" placeholder="192.168.4.1" title="Access point IP address"></td>
    </tr>
    <tr>
      <td class="label-col"><strong>Netmask:</strong></td>
      <td><input type="text" id="ntApNetmask" value="{ap_netmask}" placeholder="255.255.255.0" title="Access point subnet mask"></td>
    </tr>
  </table>
  <div style="text-align: center; margin-top: 1rem;">
    <button id="ntUp">Update Network Settings</button>
  </div>
  <div id="ntStat" class="status-message"></div>"#
    )
}

/// Generate Airfrog Settings page.
pub(crate) async fn page_settings(response: Response) -> HtmlContent {
    let sr_form = if let Some(status) = response.status {
        settings_swd_runtime(&status.settings)
    } else {
        "<p>Unable to retrieve Airfrog settings</p><br/><br/><br/>".to_string()
    };
    let st_form = settings_swd_stored().await;
    let net_form = settings_net_stored().await;

    let body = format!(
        r#"
<h1>Airfrog Settings</h1>
<div class="card">
  <h2>Network Settings</h2>
  <p>All settings in this section take effect after a reboot</p>
{net_form}
</div>
<div class="card">
  <h2>SWD Settings - Runtime</h2>
  <p>All settings in this section reset to stored config after a reboot</p>
{sr_form}
</div>
<div class="card">
  <h2>SWD Settings - Stored</h2>
  <p>All settings in this section take effect after a reboot</p>
{st_form}
</div>
<script src="{CONFIG_UPDATE_JS_PATH}"></script>
"#
    );

    HtmlContent::new(body)
}

pub fn html_timeout() -> HtmlContent {
    let body = r#"
<h1>Airfrog Timeout</h1>
<div class="card">
  <p>Airfrog is very busy right now and can't respond to you.</p> 
  <p>The binary API may be active - are you using it?</p>
  <p>Or maybe you just need to back off for a while.</p>
  <br/>
  <p>See you later!</p>
</div>"#
        .to_string();

    HtmlContent::new(body)
}

pub fn html_not_found() -> HtmlContent {
    let body = r#"
<h1>Airfrog Is Lost</h1>
<div class="card">
  <p>Airfrog can't find the page you requested.</p>
  <p>It may have been moved or deleted.</p>
  <p>Or maybe you imagined it?</p>
  <br/>
  <p>See you later!</p>
</div>"#
        .to_string();

    HtmlContent::new(body)
}

pub fn html_bad_request() -> HtmlContent {
    let body = r#"
<h1>Airfrog Is Confused</h1>
<div class="card">
  <p>Airfrog can't understand your request.</p>
  <p>Maybe you sent it something it didn't expect?</p>
  <p>Or encoded it incorrectly.</p>
  <br/>
  <p>See you later!</p>
</div>"#
        .to_string();
    HtmlContent::new(body)
}
