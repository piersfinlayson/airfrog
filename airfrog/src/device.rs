// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Contains a Device struct, with device specific information.

use alloc::format;
use alloc::string::{String, ToString};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::once_lock::OnceLock;
use esp_hal::chip;
use esp_hal::clock::Clocks;
use esp_hal::efuse::Efuse;
use esp_hal::system::Cpu;
use esp_hal::timer::systimer::{SystemTimer, Unit};

pub static DEVICE: OnceLock<Mutex<CriticalSectionRawMutex, Device>> = OnceLock::new();

/// Contains information about this specific Airfrog device.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Device {
    // Flash size in bytes
    flash_size: usize,
}

impl Device {
    /// Creates a new `Device` with the given MAC address.
    pub fn init() {
        let device = Self::default();
        DEVICE
            .init(Mutex::new(device))
            .expect("Failed to initialize DEVICE");
    }

    pub fn set_flash_size(&mut self, flash_size: usize) {
        self.flash_size = flash_size;
    }

    pub fn flash_size_bytes(&self) -> usize {
        self.flash_size
    }

    pub fn chip() -> String {
        chip!().to_string().to_ascii_uppercase()
    }

    pub fn mac_address() -> [u8; 6] {
        Efuse::read_base_mac_address()
    }

    pub fn mac_address_str() -> String {
        let mac = Self::mac_address();
        format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        )
    }

    pub fn uptime_us() -> u64 {
        let uptime_ticks = SystemTimer::unit_value(Unit::Unit0);
        uptime_ticks / (SystemTimer::ticks_per_second() / 1_000_000)
    }

    pub fn uptime_secs() -> u64 {
        Self::uptime_us() / 1_000_000
    }

    pub fn clock_speed_mhz() -> u32 {
        Clocks::get().cpu_clock.as_mhz()
    }

    pub fn heap_size() -> usize {
        crate::HEAP_SIZE
    }

    pub fn _heap_free() -> usize {
        esp_alloc::HEAP.free()
    }

    pub fn heap_used() -> usize {
        esp_alloc::HEAP.used()
    }

    pub fn reset_reason() -> String {
        let cpu = Cpu::current();
        let reset_reason = esp_hal::rtc_cntl::reset_reason(cpu);
        match reset_reason {
            Some(reason) => format!("{reason:?}"),
            None => "Unknown".to_string(),
        }
    }
}
