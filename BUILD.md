# Building airfrog

This document explains how to go about building, flashing and running the [default airfrog firmware](airfrog/README.md).

## Dependencies

Ensure you have the dependencies installed:

Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Th ESP Rust toolchain:

```bash
rustup toolchain install nightly --component rust-src
rustup target add riscv32imc-unknown-none-elf
cargo install espflash
```

See [The Rust on ESP Book](https://docs.espressif.com/projects/rust/book/) for further details.

## Building

From the repo root:

```bash
AF_STA_SSID=your-ssid AF_STA_PASSWORD=your-password cargo build --release -p airfrog
```

## Flashing

Follow this sequence:

- Hold BOOT (GP9) low
- Reset by pulling EN low briefly
- Release BOOT
- Flash it with:

```bash
AF_STA_SSID=your-ssid AF_STA_PASSWORD=your-password cargo run --release -p airfrog
```

## Running

After flashing, reboot the device with BOOT held high.  You can do this either by:
- briefly pulling EN low
- removing and reconnecting power.

You will be automatically connected to the airfrog's serial port after reset, and can view the logs.

Alternatively to connect to serial manually (best done before resetting to see boot logs):

```bash
python -m serial.tools.miniterm /dev/ttyUSB0 115200
```

Replace `/dev/ttyUSB0` with your serial port.

## Logging

To adjust log levels, set the `ESP_LOG` environment variable:

```bash
ESP_LOG=airfrog=debug AF_STA_SSID=your-ssid AF_STA_PASSWORD=your-password cargo run --release -p airfrog
```

## No WiFi

To build and run the firmware without WiFi support, disable the default features:

```bash
cargo build -p airfrog --no-default-features
```

## Debugging

As well as adjusting logging levels, you can run a development build by omitting the `--release` flag:

```bash
ESP_LOG=airfrog=debug AF_STA_SSID=your-ssid AF_STA_PASSWORD=your-password cargo run -p airfrog
```