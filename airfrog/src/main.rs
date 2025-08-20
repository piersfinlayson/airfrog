// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Default Firmware
//!
//! To use, set SSID and PASSWORD environment variables to your WiFi and then
//! build and flash the project.
//!
//! Features can be set when building to enable/disable functionality:
//! - `wifi`: Enables WiFi (and networking) support.
//! - `wifi-log`: Enables `esp-wifi` logging.
//! - `httpd`: Enables the HTTP server (requires `wifi`).
//! - `rest`: Enables the REST API (requires `httpd`).
//! - `www`: Enables the web interface (requires `httpd`).
//! - `bin-api`: Enables the binary API (requires `wifi`).
//!
//! Not all feature combinations are supported.  Some may lead to unused code
//! warnings.
//!
//! To change other configuration:
//! - `NUM_SOCKETS``: Number of sockets used by the application.  This is set
//!   automatically based on the features enabled, but can be changed below if
//!   more (or fewer) are required.
//! - `HEAP_SIZE`: Size of the heap used by the application.  This is set below
//!   and can be changed if desired.
//! - The number of httpd tasks is hard coded.  To change it in `httpd/mod.rs`:
//!   - Change `WEB_TASK_POOL_SIZE` to the number of tasks you want.
//!   - IMPORTANT - You must also add additional `spawner.must_spawn` calls
//!     in `httpd::start_httpd()` to spawn the additional tasks.  They _must
//!     not_ be created in a loop, as `make_static!` will fail to  properly
//!     create unique statics for each HttpdState.  This will cause a panic
//!     during initialization of the 2nd static.
//! - `httpd::HTTPD_PORT` specified the port that the HTTP server listens on.
//!   This can be changed in `httpd/mod.rs`.
//! - `HTTPD_TASK_TCP_RX_BUF_SIZE`, `HTTPD_TASK_TCP_TX_BUF_SIZE` and
//!   `HTTPD_TASK_BUFFER` are all set in `httpd/mod.rs` and can be changed
//!   there.
//! - There are various SWD reconnect timers/keepalives in `target/mod.rs`.
//! - `target::REQUEST_CHANNEL_SIZE` is the number of outstanding requests
//!   that can be sent to the SWD task.  This is set in `target/mod.rs`.  It
//!   should be the same or more than the number of httpd tasks.
//! - `BIN_API_TCP_RX_BUF_SIZE` and `BIN_API_TCP_TX_BUF_SIZE` are the sizes of
//!   the TCP buffers used to serve the binary API.  They are set in
//!   `bin_api/mod.rs` and can be changed there.
//! - The Target's API uses `target::BIN_API_PORT` to listen for requests.
//!   This defaults to `airfrog_bin::PORT` and can be changed in
//!   `target/mod.rs`.

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;
use core::net::{Ipv4Addr, SocketAddr};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_net::{Stack, StackResources};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer, with_deadline};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use leasehund::DhcpServer;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use static_cell::make_static;

use airfrog_util::net::{Control as WifiControl, Wifi, WifiType};

mod config;
mod device;
mod error;
mod firmware;
mod flash;
mod http;
mod macros;
mod rtt;
mod target;

use config::{CONFIG, NetMode};
use device::Device;
pub(crate) use error::{AirfrogError, ErrorKind};

include!(concat!(env!("OUT_DIR"), "/built.rs"));
pub const AIRFROG_BUILD_TIME: &str = env!("AIRFROG_BUILD_TIME");
pub const AIRFROG_BUILD_DATE: &str = env!("AIRFROG_BUILD_DATE");
pub const AIRFROG_HOME_PAGE: &str = "piers.rocks/u/airfrog";
pub const AUTHOR: &str = "Piers Finlayson";
pub const AUTHOR_EMAIL: &str = "piers@piers.rocks";

// Creates app-descriptor required by the esp-idf bootloader.  We specify
// custom values rather than using the default (), so we get the build time
// and date from the build environment variables.  Otherwise we get the build
// time and date of esp-bootloader-esp-idf.
esp_bootloader_esp_idf::esp_app_desc!(
    PKG_VERSION,
    PKG_NAME,
    AIRFROG_BUILD_TIME,
    AIRFROG_BUILD_DATE,
    esp_bootloader_esp_idf::ESP_IDF_COMPATIBLE_VERSION,
    esp_bootloader_esp_idf::MMU_PAGE_SIZE,
    0,
    u16::MAX
);

// Heap size for the application.
pub const HEAP_SIZE: usize = 128 * 1024;

// Time to wait after WiFi is up before we start checking for an IP address.
const WIFI_UP_NET_CONFIG_WAIT: Duration = Duration::from_millis(500);

/// Should only be used directly by `WrappedConfig` - otherwise use
/// CONFIG.update_flash()
pub static CONFIG_STORE_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub static REBOOT_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

// One socket required per http task, one for bin-api, one for WiFi DHCP task
// plus one spare WiFi socket.
#[cfg(feature = "bin-api")]
const NUM_SOCKETS_BIN_API: usize = 1;
#[cfg(not(feature = "bin-api"))]
const NUM_SOCKETS_BIN_API: usize = 0;
#[cfg(feature = "wifi")]
const NUM_SOCKETS_WIFI: usize = 2;
#[cfg(not(feature = "wifi"))]
const NUM_SOCKETS_WIFI: usize = 0;
#[cfg(feature = "httpd")]
pub const NUM_SOCKETS_HTTPD: usize = 4;
#[cfg(not(feature = "httpd"))]
const NUM_SOCKETS_HTTPD: usize = 0;

// Total number of sockets used by the application - add on 16 for luck.
const NUM_SOCKETS: usize = NUM_SOCKETS_BIN_API + NUM_SOCKETS_WIFI + NUM_SOCKETS_HTTPD + 16;

const WIFI_STA_TIMEOUT: Duration = Duration::from_secs(30);

// Airfrog default firmware's main function.
//
// This is kept nice and clean to make it easy to see the overall structure,
// which is:
// - Set up the HAL and device
// - Set up the heap
// - Set up wifi (if `wifi` feature is set)
// - Set up the SWD interface (`Target`)
// - Start the SWD (`Target`) task
// - Wait for WiFi connection and IP address (if `wifi` feature is set)
// - Start the HTTP server (if `httpd` feature is set)
// - Loop forever to prevent main from exiting.
#[esp_hal_embassy::main]
async fn main(spawner: Spawner) -> ! {
    //
    // Common setup code
    //

    // Set up the logger
    esp_println::logger::init_logger_from_env();

    info!("*** airfrog-swd ***");

    // Set up the HAL
    let hal_config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(hal_config);

    let clocks = esp_hal::clock::Clocks::get();
    info!(
        "Value: {} running at {}MHz",
        esp_hal::chip!(),
        clocks.cpu_clock.as_mhz()
    );

    // Set up the heap allocator
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    // Initialize embassy
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timg1.timer0);

    // Set up the device
    Device::init();

    // Load configuration into the CONFIG static
    let mut flash = flash::Flash::new().await;
    flash.load_config();

    // Set up the WiFi interface and start the WiFi connection and networking
    // tasks.  Code within this call is feature gated based on `wifi`.
    let (wifi, wifi_mode) = if cfg!(feature = "wifi") {
        // Get the STA/AP configuration
        let mut wifi_config = CONFIG.get().await.lock().await.wifi_config();
        let wifi_build = Wifi::builder();

        // Configure STA interface if required
        let wifi_build = if wifi_config.sta_if.is_some() {
            let sta_if = wifi_config.sta_if.take().unwrap();
            let stack_resources = make_static!(StackResources::<NUM_SOCKETS>::new());
            wifi_build.with_sta_if(sta_if, stack_resources)
        } else {
            if wifi_config.mode.is_sta() {
                // If the mode is StaFallbackToAp, we need a STA interface
                warn!("Error: STA mode selected, but no STA config");
            }
            wifi_build
        };

        // Configure AP interface if required
        let wifi_build = if wifi_config.ap_if.is_some() {
            let ap_if = wifi_config.ap_if.take().unwrap();
            let stack_resources = make_static!(StackResources::<NUM_SOCKETS>::new());
            wifi_build.with_ap_if(ap_if, stack_resources)
        } else {
            if wifi_config.mode.is_ap() {
                // If the mode is ApOnly, we need an AP interface
                warn!("Error: AP mode selected, but no AP config");
            }
            wifi_build
        };

        // Create the wifi object
        match wifi_build.build(
            &spawner,
            peripherals.TIMG0,
            peripherals.RNG,
            peripherals.WIFI,
        ) {
            Ok(mut wifi) => {
                trace!("Ok:    WiFi interface initialized");
                wifi.must_spawn();
                (Some(wifi), Some(wifi_config.mode))
            }
            Err(e) => {
                error!("Error: Failed to initialize WiFi interface: {e:?}");
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    // Set up the SWD interface and start the SWD task.  Always done - not
    // feature gated.  We required the network stack in order to implement
    // feature `bin-api`.
    let swdio_pin = peripherals.GPIO0;
    let swclk_pin = peripherals.GPIO1;
    let target_settings = CONFIG.get().await.lock().await.swd.into();
    let swd = target::Target::new(target_settings, swdio_pin, swclk_pin);

    // Get the sender to send requests to the Target task.
    let target_request_sender = swd.request_sender();
    let swd = make_static!(swd);

    // Get the station network stack - the binary API only runs on the STA
    // interface
    let sta_stack = if let Some(wifi) = &wifi {
        wifi.net_stack(WifiType::Sta)
    } else {
        None
    };

    // Spawn the Target task
    spawner.must_spawn(target::task(swd, sta_stack));

    if let Some(wifi_mode) = wifi_mode {
        let wifi = wifi.expect("WiFi interface should be initialized if mode is set");
        let (start_ap, started_sta) = match wifi_mode {
            NetMode::StaFallbackToAp => {
                // Enable the station and wait for the WiFi task to tell us
                // its been done
                info!("Exec:  Start WiFi station");
                wifi.control_and_wait(WifiType::Sta, WifiControl::Enable)
                    .await;

                // Next, wait for a WiFi connection and IP address before
                // continuing
                let timeout = Instant::now() + WIFI_STA_TIMEOUT;
                let failed = match with_deadline(timeout, wifi.wait_for_link_up(WifiType::Sta))
                    .await
                {
                    Ok(result) => {
                        // Timeout not reached
                        result.expect("Internal error: No STA config");
                        info!("Ok:    WiFi station connected");
                        Timer::after(WIFI_UP_NET_CONFIG_WAIT).await; // Pause before next check
                        match with_deadline(timeout, wifi.wait_for_ipv4(WifiType::Sta)).await {
                            Ok(result) => {
                                let _ = result.expect("Internal error: No STA config");
                                info!("Ok:    WiFi station IP address obtained");
                                false
                            }
                            Err(_) => {
                                warn!("Error: WiFi station IP address timed out, starting AP mode");
                                true
                            }
                        }
                    }
                    Err(_) => {
                        warn!("Error: WiFi station connection timed out, starting AP mode");
                        true
                    }
                };

                // We failed to connect via the WiFi station, so disable the
                // station and wait for it to be disabled
                if failed {
                    info!("Exec:  Disable WiFi station");
                    wifi.control_and_wait(WifiType::Sta, WifiControl::Disable)
                        .await;
                    info!("Ok:    WiFi station disabled");
                    (true, false) // Start the AP, didn't start the STA
                } else {
                    // We successfully connected via the WiFi station, so
                    // don't start the AP
                    (false, true)
                }
            }
            NetMode::ApOnly => (true, false), // Start the AP, didn't satart the STA
        };

        let started_ap = if start_ap {
            // Enable the AP and wait for it to be enabled
            info!("Exec:  Start WiFi access point");
            wifi.control_and_wait(WifiType::Ap, WifiControl::Enable)
                .await;
            true
        } else {
            false
        };

        // Start the HTTP server tasks
        if started_sta {
            http::start(
                wifi.net_stack(WifiType::Sta),
                target_request_sender,
                &spawner,
            )
            .await;
        }
        if started_ap {
            let ap_stack = wifi
                .net_stack(WifiType::Ap)
                .expect("AP stack should be available if AP is started");
            http::start(Some(ap_stack), target_request_sender, &spawner).await;

            // Start the DHCP server
            spawner.must_spawn(dhcp_task(ap_stack));

            // And the DNS server
            spawner.must_spawn(captive_dns_task(ap_stack));
        }
    }

    // Start the RTT task
    spawner.must_spawn(rtt::rtt_task(target_request_sender));

    // The main loop now turns into a flash storage task, that waits for
    // signals to store config and/or flash a new OTA image.
    //
    // TODO - Add OTA image flashing support.
    loop {
        match select(CONFIG_STORE_SIGNAL.wait(), REBOOT_SIGNAL.wait()).await {
            Either::First(_) => {
                // Config store signal received, store the config
                info!("Exec:  Store config to flash");
                flash.store_config().await;
            }
            Either::Second(_) => {
                // Reboot signal received, trigger reboot
                info!("Exec:  Rebooting in 1 second");
                Timer::after(Duration::from_secs(1)).await;
                esp_hal::system::software_reset();
            }
        }
    }
}

pub async fn create_dhcp_server() -> DhcpServer<32, 4> {
    let config_guard = CONFIG.get().await.lock().await;
    let net_cfg = &config_guard.net;

    let server_ip = Ipv4Addr::from(net_cfg.ap_v4_ip());
    let subnet_mask = Ipv4Addr::from(net_cfg.ap_v4_netmask());

    // Calculate pool range within the AP subnet
    let ap_ip = net_cfg.ap_v4_ip();
    let pool_start = Ipv4Addr::new(ap_ip[0], ap_ip[1], ap_ip[2], 100);
    let pool_end = Ipv4Addr::new(ap_ip[0], ap_ip[1], ap_ip[2], 200);

    DhcpServer::new_with_dns(
        server_ip,
        subnet_mask,
        server_ip,
        server_ip,
        pool_start,
        pool_end,
    )
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) -> ! {
    info!("Exec:  Started AP DHCP server");
    let mut dhcp_server = create_dhcp_server().await;
    dhcp_server.run(stack).await
}

#[embassy_executor::task]
async fn captive_dns_task(stack: Stack<'static>) -> ! {
    info!("Exec:  Started AP captive DNS server");
    let mut tx_buf = [0u8; 256];
    let mut rx_buf = [0u8; 256];

    let ap_ip = {
        let config_guard = CONFIG.get().await.lock().await;
        let ip_bytes = config_guard.net.ap_v4_ip();
        Ipv4Addr::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3])
    };

    // Bind to all interfaces
    let local_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 53);
    let ttl = Duration::from_secs(300);

    let udp_buffers = edge_nal_embassy::UdpBuffers::<1, 256, 256, 1>::new();
    let udp = edge_nal_embassy::Udp::new(stack, &udp_buffers);

    loop {
        debug!("Debug: Starting captive DNS server on IP: {ap_ip:?}");
        if let Err(e) = edge_captive::io::run(
            &udp,
            local_addr,
            &mut tx_buf,
            &mut rx_buf,
            ap_ip,
            ttl.into(),
        )
        .await
        {
            // Handle error, maybe log and retry
            warn!("Error: Error in captive DNS task: {e:?}");
            embassy_time::Timer::after(Duration::from_secs(1)).await;
        }
    }
}
