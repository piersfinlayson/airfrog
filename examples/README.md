# Examples

This directory contains a number of example Rust applications that demonstrate how to use the airfrog libraries to interact with SWD devices.  These allow you to start building your own airfrog applications quickly.

* [swd-basic](#swd-basic)
* [erase-stm32f4](#️erase-stm32f4)
* [gpio-stm32f4](#gpio-stm32f4)
* [sdrr-bank-switch](#sdrr-bank-switch)
* [bulk-read](#bulk-read)
* [flash-write](#️flash-write)
* [swd-speed](#swd-speed)
* [mqtt](#mqtt)
* [swd-multi-drop](#swd-multi-drop)

## Running

For example:

```bash
ESP_LOG=info cargo run --example swd-basic -p airfrog-ws
```

Replace `swd-basic` with the name of the example you want to run.

## swd-basic

* Connects to an SWD device
* Reads and reports its IDCODE
* Attempts to identify the MCU
* Attempts to read from the device's flash and RAM

## ⚠️erase-stm32f4

* Connects to an STM32F4 based target
* ⚠️Erases the device's entire flash memory

## gpio-stm32f4

* Connects to an STM32F4 based target
* Sets a GPIO as output
* Toggles the GPIO state

## sdrr-bank-switch

* Connects to an [SDRR](https://piers.rocks/u/sdrr) device, and checks it is an SDRR
* Switches the bank of the SDRR using X1/X2 pin pull ups/downs

## bulk-read

* Connects to the MCU
* Reads and outputs a chunk of data from the flash
* Reads and outputs a chunk of data from the RAM

## ⚠️flash-write

Note, if this example fails, the target device may need to be reflashed.

* Connects to an STM32F4 based target
* Backs up sector 0 (vector table and initial code)
* Erases sector 0 and writes test data (byte, half-word, word, bulk)
* Verifies all written data by reading back
* Restores original sector 0 content, leaving device functional
* Demonstrates complete flash programming cycle with proper backup/restore

## swd-speed

* Loop through speeds: Slow, Medium, Fast, Turbo
  * Connects to an SWD device
  * Attempts to read from the device's flash and RAM

## mqtt

* Connects to the target
* Reads a memory address and publishes the value to an MQTT topic once a second

## swd-multi-drop

* Detects multiple SWD targets (e.g. the RP2040/Pico)
* Connects to the first one found (should be RP2040 core 0)
