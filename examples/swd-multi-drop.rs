// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog example - Delects multi-drop SWD targets - e.g. RP2040/Pico
//!
//! To run this example:
//! - connect to the airfrog device (ESP32) using USB serial
//! - connect the airfrog device's SWD lines to the multi-drop target device
//! - reset the airfrog into bootloader mode
//! - run the example with `ESP_LOG=info cargo run --example swd-multi-drop`
//! - reset the airfrog device again to start the example

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use airfrog_core::arm::dp::TARGET_SEL_RP2040_ALL;
use airfrog_swd::debug::DebugInterface;
use airfrog_swd::protocol::Speed;

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
    swd.swd_if().set_swd_speed(Speed::Slow);

    let targets = swd
        .swd_if()
        .reset_detect_multidrop(&TARGET_SEL_RP2040_ALL)
        .await
        .expect("Failed to reset multi-drop targets");
    assert!(!targets.is_empty(), "No multi-drop targets returned"); // Guaranteed by reset_detect_multidrop()

    for target in &targets {
        info!(
            "Found target {} {} (ID: {})",
            target.name(),
            target.target(),
            target.idcode()
        );
    }

    swd.swd_if()
        .reset_multidrop_target(&targets[0])
        .await
        .expect("Failed to reset first multi-drop target");
    info!(
        "First target {:?} {:?}",
        swd.swd_if().idcode(),
        swd.swd_if().mcu()
    );

    // Try and identify the target MCU
    let mcu = swd.mcu().expect("Failed to get MCU details");
    info!("Target MCU: {mcu:#}");

    // Read memory from the target device
    let target_addr = [
        (Some(0x4000_0000), "Chip ID"),
        (Some(0xE000_ED00), "CPU ID"),
        (Some(0x4000_0004), "Platform"),
        (Some(0x4000_0040), "Gitref_RP2040"),
        (Some(0x0000_0010), "Bootrom Magic/Version"),
        (mcu.flash_base(), "Flash Base"),
        (mcu.ram_base(), "RAM Base"),
    ];
    for addr in target_addr {
        if let (Some(addr), name) = addr {
            match swd.read_mem(addr).await {
                Ok(data) => info!("Read word from {name} at 0x{addr:08X}: 0x{data:08X}"),
                Err(e) => panic!("Failed to read from {name} at 0x{addr:08X}: {e:?}"),
            }
        } else {
            info!("No valid address to read from - skipping");
            continue;
        }
    }

    // We're done
    info!("Example complete");

    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}
