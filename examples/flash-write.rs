// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog example - Writes test data to an STM32F4's flash sector 0 and
//! restores it when complete
//!
//! This example demonstrates flash programming by backing up sector 0,
//! erasing it, writing test data, verifying the writes, then restoring the
//! original content.  The target device should be fully functional after
//! completion.
//!
//! To run this example:
//! - connect to the airfrog device (ESP32) using USB serial
//! - connect the airfrog device's SWD lines to the target device
//! - reset the airfrog into bootloader mode
//! - run the example with `ESP_LOG=info cargo run --example flash-write`
//! - reset the airfrog device again to start the example

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;
use alloc::vec;

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use log::{error, info, warn};

use airfrog_swd::debug::DebugInterface;

// Creates app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

// Heap size for the application - need extra space for sector backup
const HEAP_SIZE: usize = 128 * 1024;

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

    // Try and identify the target MCU
    let mcu = swd.mcu().expect("Failed to get MCU details");
    info!("Target: {mcu}");

    let flash_base = mcu.flash_base().expect("No flash base address found");
    info!("Flash base address: 0x{flash_base:08X}");

    // Step 1: Read and backup sector 0
    info!("Reading sector 0 for backup...");
    let sector0_size_words = mcu
        .get_sector_size_words(0)
        .expect("Failed to get sector size for sector 0");
    let mut sector0_backup = vec![0u32; sector0_size_words as usize];
    swd.read_mem_bulk(flash_base, &mut sector0_backup, true)
        .await
        .unwrap_or_else(|(e, _)| panic!("Failed to read sector 0: {e:?}"));
    info!("✓ Sector 0 backed up ({} words)", sector0_backup.len());

    // Unlock the flash
    swd.unlock_flash()
        .await
        .unwrap_or_else(|e| panic!("Failed to unlock flash memory: {e:?}"));
    info!("✓ Flash unlocked");

    // Step 2: Erase sector 0
    info!("Erasing sector 0...");
    swd.erase_sector(0)
        .await
        .unwrap_or_else(|e| panic!("Failed to erase sector 0: {e:?}"));
    info!("✓ Sector 0 erased");

    // Step 3: Write test data
    info!("Writing test data...");

    // Test byte write
    let test_byte: u8 = 0x42;
    swd.write_flash_u8(flash_base, test_byte)
        .await
        .unwrap_or_else(|e| panic!("Failed to write byte to flash: {e:?}"));
    let read_byte = swd.read_mem(flash_base).await.unwrap() as u8;
    if read_byte == test_byte {
        info!("✓ Byte write successful: 0x{test_byte:02X}");
    } else {
        panic!("Byte write failed: wrote 0x{test_byte:02X}, read 0x{read_byte:02X}");
    }

    // Test half-word write
    let test_halfword: u16 = 0x1234;
    let halfword_addr = flash_base + 0x10;
    swd.write_flash_u16(halfword_addr, test_halfword)
        .await
        .unwrap_or_else(|e| panic!("Failed to write half-word to flash: {e:?}"));
    let read_halfword = swd.read_mem(halfword_addr).await.unwrap() as u16;
    if read_halfword == test_halfword {
        info!("✓ Half-word write successful: 0x{test_halfword:04X}");
    } else {
        panic!("Half-word write failed: wrote 0x{test_halfword:04X}, read 0x{read_halfword:04X}");
    }

    // Test word write
    let test_word: u32 = 0xDEADBEEF;
    let word_addr = flash_base + 0x20;
    swd.write_flash_u32(word_addr, test_word)
        .await
        .unwrap_or_else(|e| panic!("Failed to write word to flash: {e:?}"));
    let read_word = swd.read_mem(word_addr).await.unwrap();
    if read_word == test_word {
        info!("✓ Word write successful: 0x{test_word:08X}");
    } else {
        panic!("Word write failed: wrote 0x{test_word:08X}, read 0x{read_word:08X}");
    }

    // Test bulk write
    let test_data = [0x12345678, 0x9ABCDEF0, 0xFEDCBA98, 0x76543210];
    let bulk_addr = flash_base + 0x100;
    swd.write_flash_bulk(bulk_addr, &test_data)
        .await
        .unwrap_or_else(|e| panic!("Failed to write bulk data to flash: {e:?}"));

    // Verify bulk write
    let mut read_data = [0u32; 4];
    swd.read_mem_bulk(bulk_addr, &mut read_data, true)
        .await
        .unwrap_or_else(|(e, _)| panic!("Failed to read bulk data from flash: {e:?}"));

    if read_data == test_data {
        info!("✓ Bulk write successful: {} words", test_data.len());
    } else {
        panic!("Bulk write failed: data mismatch");
    }

    // Step 4: Erase sector 0 again to prepare for restore
    info!("Erasing sector 0 for restore...");
    swd.erase_sector(0)
        .await
        .unwrap_or_else(|e| panic!("Failed to erase sector 0 for restore: {e:?}"));
    info!("✓ Sector 0 erased for restore");

    // Step 5: Restore original sector 0 content
    info!("Restoring original sector 0 content...");
    swd.write_flash_bulk(flash_base, &sector0_backup)
        .await
        .unwrap_or_else(|e| panic!("Failed to restore sector 0: {e:?}"));
    info!("✓ Sector 0 restored ({} words)", sector0_backup.len());

    // Step 6: Verify restoration
    info!("Verifying restoration...");
    let mut sector0_verify = vec![0u32; sector0_size_words as usize];
    swd.read_mem_bulk(flash_base, &mut sector0_verify, true)
        .await
        .unwrap_or_else(|(e, _)| panic!("Failed to read sector 0 for verification: {e:?}"));

    if sector0_verify == sector0_backup {
        info!("✓ Sector 0 restoration verified successfully");
    } else {
        warn!("⚠️  Sector 0 restoration verification failed - device may not function correctly");
        for (i, (&original, &restored)) in
            sector0_backup.iter().zip(sector0_verify.iter()).enumerate()
        {
            if original != restored {
                warn!(
                    "Mismatch at offset 0x{:04X}: original 0x{:08X}, restored 0x{:08X}",
                    i * 4,
                    original,
                    restored
                );
                break;
            }
        }
    }

    // Lock the flash again
    swd.lock_flash()
        .await
        .unwrap_or_else(|e| panic!("Failed to lock flash memory: {e:?}"));
    info!("✓ Flash locked");

    // We're done
    info!("Example completed successfully! Target device should be fully functional.");

    loop {
        Timer::after_secs(1).await;
    }
}
