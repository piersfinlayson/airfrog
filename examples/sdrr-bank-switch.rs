// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog example - Rotates between a Software Defined Retro ROM's bank of
//! ROM images, by toggling pins X1/X2.
//!
//! Find out more about SDRR at [piers.rocks/u/sdrr](https://piers.rocks/u/sdrr).
//!
//! To run this example:
//! - connect to the airfrog device (ESP32) using USB serial
//! - connect the airfrog device's SWD lines to the SDRR
//! - reset the airfrog into bootloader mode
//! - run the example with `ESP_LOG=info cargo run --example erase-stm32f4`
//! - reset the airfrog device again to start the example

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use log::{debug, error, info};

use airfrog_core::Mcu;
use airfrog_core::stm::{STM32F4_GPIOC_PUPDR, STM32F4_PUPDR_PD, STM32F4_PUPDR_PU};
use airfrog_swd::SwdError;
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
    let mcu = loop {
        match swd.reset_swd_target().await {
            Ok(()) => info!(
                "Connected to target with ID code: {}",
                swd.idcode().unwrap()
            ),
            Err(e) => {
                error!("Failed to connect to target: {e:?}");
                info!("Retrying shortly ...");
                Timer::after_millis(500).await;
                continue;
            }
        };

        // Try and identify the target MCU
        match swd.mcu() {
            Some(mcu) => match mcu {
                Mcu::Stm32(stm) => {
                    info!("Target is an STM32F4");
                    debug!("{stm}");
                    break mcu;
                }
                _ => {
                    error!("Target MCU is not an STM32F4: {mcu}");
                    loop {
                        Timer::after_secs(5).await;
                    }
                }
            },
            None => {
                info!("Retrying shortly ...");
                Timer::after_millis(500).await;
                continue;
            }
        }
    };

    // Check for the magic bytes in flash
    const SDRR_MAGIC_BYTE_OFFSET: u32 = 0x200;
    let magic_bytes = swd
        .read_mem(mcu.flash_base().unwrap() + SDRR_MAGIC_BYTE_OFFSET)
        .await
        .expect("Failed to read magic bytes from flash");
    if magic_bytes.to_le_bytes() != *b"SDRR" {
        panic!("Didn't find SDRR magic bytes: 0x{magic_bytes:08X}. Is this an SDRR device?");
    }
    info!("Detected Software Defined Retro ROM");

    // Main loop.  We need to re-initialize the device if `main_loop()`
    // returns an error, before starting again.
    let mut reset = false;
    loop {
        if reset {
            // Reset the target device
            info!("Resetting target device...");
            match swd.reset_swd_target().await {
                Ok(_) => reset = true,
                Err(e) => {
                    error!("Failed to reset target: {e:?}");
                    info!("Retrying shortly ...");
                    Timer::after_millis(500).await;
                    continue;
                }
            }
        }

        match main_loop(&mut swd).await {
            Ok(_) => (),
            Err(e) => {
                error!("Error in main loop: {e:?}");
                reset = true;
            }
        }
    }
}

async fn main_loop<'a>(swd: &mut DebugInterface<'a>) -> Result<(), SwdError> {
    // Set up GPIO constants
    const GPIO_PUPDR: u32 = STM32F4_GPIOC_PUPDR;
    const X1_PIN: u32 = 14;
    const X2_PIN: u32 = 15;

    // Do pull-up/pull-down loop
    const MS_TIMER: u64 = 1000;
    let mut count = 0;
    info!("Started bank switching at {MS_TIMER}ms intervals");
    loop {
        if count > 3 {
            count = 0
        };
        let (x1, x2) = match count {
            0 => (STM32F4_PUPDR_PD, STM32F4_PUPDR_PD),
            1 => (STM32F4_PUPDR_PU, STM32F4_PUPDR_PD),
            2 => (STM32F4_PUPDR_PD, STM32F4_PUPDR_PU),
            3 => (STM32F4_PUPDR_PU, STM32F4_PUPDR_PU),
            _ => unreachable!(),
        };

        // Set pull-up/pull-down for X1 and X2 pins
        let mut pupdr = swd.read_mem(GPIO_PUPDR).await?;
        pupdr &= !(0b11 << (X1_PIN * 2));
        pupdr &= !(0b11 << (X2_PIN * 2));
        pupdr |= x1 << (X1_PIN * 2);
        pupdr |= x2 << (X2_PIN * 2);
        swd.write_mem(GPIO_PUPDR, pupdr).await?;

        // Pause before next iteration
        Timer::after_millis(MS_TIMER).await;
        count += 1;
    }
}
