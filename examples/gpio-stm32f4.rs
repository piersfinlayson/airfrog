// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog example - Toggles an STM32F4's GPIO
//!
//! To run this example:
//! - connect to the airfrog device (ESP32) using USB serial
//! - connect the airfrog device's SWD lines to the target device
//! - reset the airfrog into bootloader mode
//! - run the example with `ESP_LOG=info cargo run --example gpio-stm32f4`
//! - reset the airfrog device again to start the example

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use log::{error, info};

use airfrog_core::Mcu;
use airfrog_core::stm::{
    STM32F4_GPIOB_BSRR, STM32F4_GPIOB_MODER, STM32F4_MODER_MASK, STM32F4_MODER_OUTPUT,
};
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

    // Try and identify the target MCU
    let mcu = swd.mcu().expect("Failed to get MCU details");
    match mcu {
        Mcu::Stm32(stm) => info!("Target is an STM32F4: {stm}"),
        _ => {
            error!("Target MCU is not an STM32F4: {mcu}");
            loop {
                Timer::after_secs(5).await;
            }
        }
    }

    // Set up pin constants for the STM32F4
    const MODER_ADDR: u32 = STM32F4_GPIOB_MODER;
    const BSRR_ADDR: u32 = STM32F4_GPIOB_BSRR;
    const GPIO_PIN: u32 = 15;

    // Set pin as output
    let mut moder = swd
        .read_mem(MODER_ADDR)
        .await
        .expect("Failed to read MODER");
    moder &= !(STM32F4_MODER_MASK << (GPIO_PIN * 2)); // Clear the mode bits for the pin
    moder |= STM32F4_MODER_OUTPUT << (GPIO_PIN * 2); // Set the pin to output mode
    swd.write_mem(MODER_ADDR, moder)
        .await
        .expect("Failed to set pin mode");

    // Toggle GPIO pin
    info!("Toggling GPIO...");
    loop {
        // Set pin high
        let bsrr = 1 << GPIO_PIN;
        swd.write_mem(BSRR_ADDR, bsrr)
            .await
            .expect("Failed to set pin high");
        Timer::after_millis(100).await;

        // Set pin low
        let bsrr = 1 << (GPIO_PIN + 16);
        swd.write_mem(BSRR_ADDR, bsrr)
            .await
            .expect("Failed to set pin low");
        Timer::after_millis(100).await;
    }
}
