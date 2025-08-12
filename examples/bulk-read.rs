// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog example - Bulk memory reading over SWD
//!
//! This example demonstrates high-performance bulk memory operations using
//! the read_drw_multi function for efficient data transfer.
//!
//! To run this example:
//! - connect to the airfrog device (ESP32) using USB serial
//! - connect the airfrog device's SWD lines to the target device
//! - reset the airfrog into bootloader mode
//! - run the example with `ESP_LOG=info cargo run --example swd-bulk-read`
//! - reset the airfrog device again to start the example

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

use alloc::string::String;
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use log::{error, info};

use airfrog_swd::debug::DebugInterface;

// Creates app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

// Heap size for the application.
const HEAP_SIZE: usize = 64 * 1024;

// Number of 32-bit words to read in bulk operations
const BULK_READ_SIZE: usize = 64;

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) -> ! {
    // Set up the logger - use ESP_LOG env variable to control log level, e.g.
    // ESP_LOG=info cargo run --example swd-bulk-read
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

    // Try and identify the target MCU
    let (ram_base, flash_base) = if let Some(mcu) = swd.mcu() {
        info!("Target MCU: {mcu}");
        (
            mcu.ram_base().expect("Failed to get RAM base"),
            mcu.flash_base().expect("Failed to get Flash base"),
        )
    } else {
        panic!("Failed to identify target MCU");
    };

    // Read the vector table (first 64 words of flash)
    info!("Reading first {BULK_READ_SIZE} words from flash at 0x{flash_base:08X}...");

    let mut data = [0u32; BULK_READ_SIZE];
    let start_time = Instant::now();
    match swd.read_mem_bulk(flash_base, &mut data, true).await {
        Ok(()) => {
            let end_time = Instant::now();
            info!(
                "Successfully read {} words from flash in {:?}",
                data.len(),
                end_time.duration_since(start_time)
            );
            display_memory_dump("Vector Table", flash_base, &data);
        }
        Err((e, count)) => {
            error!("Bulk read failed after {count} words: {e:?}");
        }
    }

    // Read a chunk of RAM to show it works there too
    info!("Reading {BULK_READ_SIZE} words from RAM at 0x{ram_base:08X}...");

    let mut data = [0u32; BULK_READ_SIZE];
    let start_time = Instant::now();
    match swd.read_mem_bulk(ram_base, &mut data, true).await {
        Ok(()) => {
            let end_time = Instant::now();
            let duration = end_time.duration_since(start_time);
            info!(
                "Successfully read {} words from RAM in {:?}",
                data.len(),
                duration
            );
            display_memory_dump("RAM Contents", ram_base, &data);
        }
        Err((e, count)) => {
            error!("RAM bulk read failed after {count} words: {e:?}");
        }
    }

    info!("Bulk read example completed successfully!");

    loop {
        Timer::after_secs(1).await;
    }
}

fn display_memory_dump(title: &str, base_addr: u32, data: &[u32]) {
    info!("{title}:");
    for (i, chunk) in data.chunks(4).enumerate() {
        let addr = base_addr + (i * 16) as u32;
        let mut line = String::new();
        let _ = core::fmt::write(&mut line, format_args!("  0x{addr:08X}:"));

        for word in chunk {
            let _ = core::fmt::write(&mut line, format_args!(" {word:08X}"));
        }

        info!("{}", line.as_str());
    }
}
