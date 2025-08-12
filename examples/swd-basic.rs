// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog example - Connect to a device over SWD
//!
//! Supports v1, v2 and v2 multi-drop targets, including
//! - STM32F4
//! - RP2040
//! - RP2350
//!
//! To run this example:
//! - connect to the airfrog device (ESP32) using USB serial
//! - connect the airfrog device's SWD lines to the target device
//! - reset the airfrog into bootloader mode
//! - run the example with `ESP_LOG=info cargo run --example swd-basic`
//! - reset the airfrog device again to start the example

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use log::{error, info};

use airfrog_swd::debug::DebugInterface;

// Creates app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

// Heap size for the application.
const HEAP_SIZE: usize = 64 * 1024;

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) -> ! {
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

    // Create the DebugInterface, which is how we'll drive the target
    let swdio_pin = peripherals.GPIO0;
    let swclk_pin = peripherals.GPIO1;
    let mut swd = DebugInterface::from_pins(swdio_pin, swclk_pin);

    // Keep trying to connect to the target device until we succeed
    loop {
        match swd.reset_swd_target().await {
            Ok(()) => {
                info!(
                    "Connected to target with ID code: {}",
                    swd.idcode().unwrap()
                );
                break;
            }
            Err(e) => {
                error!("Failed to connect to target: {e:?}");
                info!("Retrying shortly ...");
                Timer::after_secs(5).await;
                continue;
            }
        }
    }

    info!(
        "Using SWD version: {:?}",
        swd.swd_if().check_version().unwrap()
    );

    // Try and identify the target MCU
    let mcu = swd.mcu().expect("Failed to get MCU details");
    info!("Target MCU: {mcu:#}");

    // Read memory from the target device
    let target_addr = [mcu.flash_base(), mcu.ram_base()];
    for addr in target_addr {
        if let Some(addr) = addr {
            match swd.read_mem(addr).await {
                Ok(data) => info!("Read byte from target memory at 0x{addr:08X}: 0x{data:08X}"),
                Err(e) => panic!("Failed to read memory at 0x{addr:08X}: {e:?}"),
            }
        } else {
            info!("No valid address to read from - skipping");
            continue;
        }
    }

    // We're done
    info!("Example completed successfully!");

    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}
