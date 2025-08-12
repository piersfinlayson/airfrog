# Technical Details

Contents:

- [Overview - Co-processing with SWD](#overview---co-processing-with-swd)
- [Airfrog SWD Implementation](#airfrog-swd-implementation)
- [Rust](#rust)
- [ESP32-C3](#esp32-c3)
- [SWD Protocol](#swd-protocol)
- [Partitions](#partitions)

## Overview - Co-processing with SWD

Serial Wire Debug (SWD) is a protocol typically used for debugging and programming ARM Cortex-M microcontrollers.  SWD is widely supported by many, although not all ARM devices - it is a vendor's implementation decision to support.  Some notable devices that support SWD include the STM32 series (at least the STM32F1 and STM32F4), and the Raspberry Pi Pico/Pico 2.

SWD supports various operations, including:
- Reading and writing memory
- Reading and writing hardware registers
- Reading, erasing and programming flash memory
- Halting and resuming execution
- Reading and writing CPU registers
- Setting breakpoints
- Single stepping through code

As well as debugging and programming, which SWD was designed to do, we can also use SWD to allow a co-processor to interface directly to the target device, influence the target device and behaviour, augment its capabilities, providing additional functionality.  This is the airfrog's purpose - to be that co-processor, alongside being a more traditional programmer and debugger.

## Airfrog SWD Implementation

If you are unfamilar with SWD, you may want to read [SWD](./SWD.md) first.

Airfrog bit-bangs the SWD protocol, using two GPIOs.  It can be configured to operate at a number of different clock speeds - all figues are approximate:
| Speed   | Frequency |
|---------|-----------|
| Slow    | 500 KHz   |
| Medium  | 1 MHz     |
| Fast    | 2 MHz     |
| Turbo*  | 4 MHz     |

\* Default

Turbo is the max speed which can be achieved using bitbanging and running the ESP32-C3 at 160MHz.  It should be reliable if airfrog and the target device are directly connected or connected with short wires.  If you are using long wires, or the target device is noisy, you may need to use a slower speed to get reliable communication.

Airfrog's SWD clock is not entirely symmetric - typically the clock is high slightly longer than it is low, simply due to the work that is being done in the high phase.  This does not lead to any problems in practice.

It might be feasible to get slightly higher performance out of the ESP32-C3 by using its SPI peripheral, but it is unlikely to give much of an improvement beyond bit-banging.

## Rust

Rust was chosen as the basis for airfrog because:
- It now has good support on the ESP32 hardware, particularly the RISC-V variants such as the ESP32-C3.
- Elements of [Software Defined Retro ROM](https://piers.rocks/u/sdrr) are implemented in Rust, and SDRR was a key target for airfrog's initial implementation.
- Rust and [embassy-rs](embassy.rs) provide a good framework for writing asynchronous code, which is useful for airfrog's various tasks.
- It has strong tooling and a wealth of libraries (crates).

The `esp-hal` framework was chosen as opposed to `esp-idf`, because the overhead of the `esp-idf`'s RTOS was considered unnecessary.

Airfrog is written as `no_std`, with `alloc` (specifically `esp-alloc`) for heap management and allocation/de-allocation.

Airfrog's list of crate dependencies is deliberatey kept relatively low, beyond the various `esp` and `embassy` crates.

## ESP32-C3

The ESP32-C3 was chosen as the MCU for airfrog because:
- It is powerful, with a 160MHz RISC-V core, 400KB of RAM, and 4MB of flash.
- It has good support for Rust, particularly with the esp-hal and embassy-rs.
- It provides WiFi support for remote access to the target.
- It is inexpensive, typically costing around $3.  This allows the entire airfrog BOM cost to come in under well-under $5 per unit, even in very low quanties.

## Partitions

Airfrog uses the following partition scheme:

```ascii
╭────────────────┬──────┬─────────┬──────────┬────────────────────┬───────────╮
│ Name           ┆ Type ┆ SubType ┆ Offset   ┆ Size               ┆ Encrypted │
╞════════════════╪══════╪═════════╪══════════╪════════════════════╪═══════════╡
│ nvs            ┆ data ┆ nvs     ┆ 0x9000   ┆ 0x4000 (16KiB)     ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ otadata        ┆ data ┆ ota     ┆ 0xd000   ┆ 0x2000 (8KiB)      ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ phy_init       ┆ data ┆ phy     ┆ 0xf000   ┆ 0x1000 (4KiB)      ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ airfrog_fw_0   ┆ app  ┆ ota_0   ┆ 0x10000  ┆ 0x180000 (1536KiB) ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ airfrog_data_0 ┆ data ┆ spiffs  ┆ 0x190000 ┆ 0x70000 (448KiB)   ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ airfrog_conf_0 ┆ data ┆ fat     ┆ 0x200000 ┆ 0x8000 (32KiB)     ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ airfrog_conf_1 ┆ data ┆ fat     ┆ 0x208000 ┆ 0x8000 (32KiB)     ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ airfrog_fw_1   ┆ app  ┆ ota_1   ┆ 0x210000 ┆ 0x180000 (1536KiB) ┆           │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌┤
│ airfrog_data_1 ┆ data ┆ spiffs  ┆ 0x390000 ┆ 0x70000 (448KiB)   ┆           │
╰────────────────┴──────┴─────────┴──────────┴────────────────────┴───────────╯

This table can be regenerated using `espflash partition-table airfrog/partitions.csv`.

When developing/debugging, individual partitions can be cleared with:

```bash
espflash erase-parts --partition-table airfrog/partitions.csv airfrog_conf_0
```