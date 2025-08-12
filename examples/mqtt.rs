// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog example - Read a memory address and publish to MQTT every second
//!
//! Specifically, this example works with the Software Defined Retro ROM (SDRR)
//! project.  When SDRR is built with COUNT_ROM_ACCESS enabled, it updates a
//! counter in RAM every second with the total number of ROM accesses since
//! boot.  This example is designed to read that counter every second, and
//! output the number of accesses, in the last second, to an MQTT broker.
//!
//! To run this example:
//! - connect to the airfrog device (ESP32) using USB serial
//! - connect the airfrog device's SWD lines to the target device
//! - reset the airfrog into bootloader mode
//! - run the example by exporting the environment variables with your values
//!   (the exaples will publish to airfrog/{airfrog_id}/counter):
//!   `SSID=ssid PASSWORD=password MQTT_BROKER_IP=1.2.3.4 AIRFROG_ID=1 ESP_LOG=info cargo run --example mqtt`
//! - reset the airfrog device again to start the example
//!
//! See `scripts/graph_mqtt.py` for a script that can be used to graph the
//! published data.

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

use alloc::string::{String, ToString};
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::{Ipv4Address, StackResources, tcp::TcpSocket};
use embassy_time::{Duration, Instant, Timer};
use esp_alloc as _;
use esp_alloc as _;
use esp_backtrace as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use log::{error, info, warn};
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::publish_packet::QualityOfService::QoS1,
    utils::rng_generator::CountingRng,
};
use static_cell::make_static;

use airfrog_swd::debug::DebugInterface;
use airfrog_util::net::{Control as WifiControl, InterfaceConfig, Wifi, WifiType};

// Creates app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

// Address in RAM we expect to find some magic bytes (should be SDRQ if the
// target is a Software Defined Retro ROM).
const MAGIC_ADDR: u32 = 0x2000_0000;

// The address in the target's RAM we will read from.
const RAM_ADDR: u32 = 0x2000_0008;

// Heap size for the application.
const HEAP_SIZE: usize = 64 * 1024;

// MQTT broker address and port
const MQTT_BROKER_IP: &str = env!("MQTT_BROKER_IP");

// Strings for MQTT
const AIRFROG_CLIENT_ID: &str = concat!("airfrog", env!("AIRFROG_ID"));
const AIRFROG_TOPIC: &str = concat!("airfrog/", env!("AIRFROG_ID"), "/counter");

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) -> ! {
    // Set up the logger - use ESP_LOG env variable to control log level, e.g.
    // ESP_LOG=info cargo run --example <example_name>
    esp_println::logger::init_logger_from_env();

    // Set up the heap allocator - required for logging and string handling
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    // Set up the HAL
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize embassy
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timg1.timer0);

    // Set up the WiFi interface and start the WiFi connection and networking
    // tasks.
    let sta_stack_resources = make_static!(StackResources::<2>::new());
    let sta_config = InterfaceConfig {
        ssid: String::from(env!("AF_STA_SSID")),
        password: String::from(env!("AF_STA_PASSWORD")),
        net: embassy_net::Config::dhcpv4(Default::default()),
    };
    let mut wifi = Wifi::builder::<2, 0>()
        .with_sta_if(sta_config, sta_stack_resources)
        .build(
            &spawner,
            peripherals.TIMG0,
            peripherals.RNG,
            peripherals.WIFI,
        )
        .expect("Failed to initialize WiFi");
    wifi.must_spawn();
    wifi.control_and_wait(WifiType::Sta, WifiControl::Enable)
        .await;

    // Create the DebugInterface, which is how we'll drive the target
    let swdio_pin = peripherals.GPIO0;
    let swclk_pin = peripherals.GPIO1;
    let mut swd = DebugInterface::from_pins(swdio_pin, swclk_pin);

    // Wait for WiFi connection and IP address before continuing
    let _ = wifi.wait_for_ipv4(WifiType::Sta).await;

    // Create an MQTT socket
    let mut rx_buf = [0; 256];
    let mut tx_buf = [0; 256];
    let stack = wifi
        .net_stack(WifiType::Sta)
        .expect("Failed to set up networking - exiting");
    let mut mqtt_socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);

    // And connect to the MQTT broker
    let mut mqtt_rx_buf = [0; 64];
    let mut mqtt_tx_buf = [0; 64];
    let broker_ip = Ipv4Address::from_str(MQTT_BROKER_IP).unwrap();
    let remote_endpoint = (broker_ip, 1883);
    mqtt_socket
        .connect(remote_endpoint)
        .await
        .expect("Failed to connect to MQTT broker");

    info!("Connected via TCP to MQTT broker at {MQTT_BROKER_IP}");

    // Create the MQTT client
    let mut mqtt_config = ClientConfig::new(
        rust_mqtt::client::client_config::MqttVersion::MQTTv5,
        CountingRng(20000),
    );
    mqtt_config.add_client_id(AIRFROG_CLIENT_ID);
    mqtt_config.max_packet_size = 128;
    let mut client = MqttClient::<_, 5, _>::new(
        mqtt_socket,
        &mut mqtt_tx_buf,
        64,
        &mut mqtt_rx_buf,
        64,
        mqtt_config,
    );

    client
        .connect_to_broker()
        .await
        .expect("Failed to connect to MQTT broker");

    while swd.reset_swd_target().await.is_err() {
        warn!("Failed to reset SWD target, retrying...");
        Timer::after_secs(1).await;
    }

    // Read 0x2000_0000.
    // This checks whether the target is a Software Defined Retro ROM, which
    // has a counter at RAM_ADDR that it will increment every second with the
    // total number of ROM accesses since boot.
    let word = swd
        .read_mem(MAGIC_ADDR)
        .await
        .expect("Failed to read magic address");
    if word != 0x72726473 {
        warn!("Expected magic word 0x72726473 at 0x{MAGIC_ADDR:08X}, but found 0x{word:08X}");
    } else {
        info!("Found expected magic word 0x72726473 (sdrr) at 0x{MAGIC_ADDR:08X}");

        // 4th byte of the next word indicates whether ROM access counting is
        // enabled
        let word = swd
            .read_mem(MAGIC_ADDR + 4)
            .await
            .expect("Failed to read magic address + 4");
        let byte = word.to_le_bytes()[3];
        if byte == 0 {
            warn!("ROM access counting is not enabled (0x{word:08X} at 0x{MAGIC_ADDR:08X})");
        } else {
            info!("ROM access counting is enabled (0x{word:08X} at 0x{MAGIC_ADDR:08X})");
        }
    }

    // Loop forever, reading from the target's RAM and publishing to MQTT
    let mut next_measurement = Instant::now();
    let mut last_measurement: Option<u32> = None;
    loop {
        // This tries to read in exactly 1 second intervals, although there
        // will be variation, on average it should be close.
        next_measurement += Duration::from_secs(1);
        Timer::at(next_measurement).await;
        let current_measurement = match swd.read_mem(RAM_ADDR).await {
            Ok(word) => word,
            Err(e) => {
                error!("Failed to read memory at 0x{RAM_ADDR:08X}: {e:?}");
                continue;
            }
        };
        info!("Read word from target memory at 0x{RAM_ADDR:08X}: 0x{current_measurement:08X}");

        // If we have already read a measurement, calculate the difference and
        // publish it to the MQTT broker.
        if let Some(last) = last_measurement {
            let since_last = current_measurement.wrapping_sub(last);
            info!("Counter increment since last measurement: {since_last}");

            let since_last_str = since_last.to_string();
            match client
                .send_message(AIRFROG_TOPIC, since_last_str.as_bytes(), QoS1, false)
                .await
            {
                Ok(_) => info!("Published message: {since_last_str}"),
                Err(e) => error!("Failed to publish message: {e:?}"),
            }
        }

        // Update last measurement, including in the first iteration
        last_measurement = Some(current_measurement);
    }
}
