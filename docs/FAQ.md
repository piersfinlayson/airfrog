# Airfrog Frequently Asked Questions

## What is Airfrog?

**Q: What exactly is Airfrog?**

A: Three things:

* A tiny, wireless, $3 device that can unobstrusively directly access flash, RAM and hardware peripherals of ARM devices.
* A set of libraries and a framework for quickly building custom applications to access and control ARM devices.
* A wireless debugger/programmer for ARM devices that can be used with existing tools like [probe-rs](https://probe.rs/).

**Q: What can I do with Airfrog?**

A: The limits are your imagination. Some examples:

* **Remote Telemetry**: Monitor RAM and flash of a running, embedded device over WiFi.
* **Add Networking**: Extend an existing unconnected ARM device with WiFi capabilities, with no or minimal CPU cycles from target.
* **Remote Programming**: Program ARM devices over WiFi, without physical access to the target device.
* **Add Features**: Add new features to an existing ARM device, without modifying the target device, but instead accessing its peripherals remotely, in parallel to the target.

## Hardware and Compatibility

**Q: What ARM devices does Airfrog support?**

A: Airfrog works with any ARM device that supports the SWD (Serial Wire Debug) protocol. This includes:

* **STM32 series**: STM32F1, STM32F4 (tested and confirmed working)
* **Raspberry Pi Pico/Pico 2**: Full support
* **Most ARM Cortex-M microcontrollers**: Check your device documentation for SWD support

Note that SWD support is a vendor implementation decision, so not all ARM devices include it.  The firmware must have enabled SWD (or, at least, not disabled it), and the hardware exposed the SWD pins, for Airfrog to work.

**Q: How big is Airfrog and what does it cost?**

A: Airfrog is tiny - about the size of a postage stamp or quarter (16×28mm). The bill of materials costs around $3 using an ESP32-C3-MINI-1 module, making the complete device under $5 even in very low quantities.

**Q: What connections does Airfrog need to the target?**

A: Airfrog connects to your target ARM device using a 5-pin 0.1" pitch connector:
* **Power**: 5V input with onboard 3.3V regulator
* **SWD**: SWDIO and SWCLK pins (3.3V levels)
* **Ground**: Common ground connection

The 5th pin is reserved for future use (such as controlling the target's reset line) and disconnedcted in the current rev a hardware.

**Q: What connections does Airfrog need for programming?**

A: Airfrog can be programmed using serial (for example from a PC using USB serial). It exposes a separate UART for programming, allowing you to flash new firmware without needing to disassemble the device.  There are 6 additional 0.1" pitch programming pins:
* **UART**: TX and RX for programming
* **GND**: Common ground
* **VCC**: 5V power input
* **EN**: Used to reset Airfrog for programming
* **IO9/BOOT**: Used to enter Airfrog bootloader mode for programming

## Getting Started

**Q: How do I get started with Airfrog?**

A: The quickest path:

1. **Get hardware**: Either build your own PCB, using the [PCB design](pcb/README.md), or source one pre-made.
2. **Install dependencies**: Follow the [build instructions](BUILD.md)
3. **Flash firmware**: Connect via USB serial and run `SSID=your-ssid PASSWORD=your-password cargo run -p airfrog`
4. **Access web interface**: Point your browser to `http://airfrog-ip/`.  Airfrog sources its IP address via DHCP, so you can find it in your router's DHCP client list.

**Q: Do I need to know Rust to use Airfrog?**

A: Not necessarily:

* **For basic use**: The default firmware provides web and REST APIs - no Rust required
* **For custom applications**: Yes, applications are built using Rust and the provided libraries
* **For high performance integration**: The Binary API can be used from any language that supports TCP/IP

**Q: Can I use Airfrog with my existing debugging tools?**

A: Yes! Airfrog integrates with:

* **probe-rs**: Use the forked version for wireless debugging and programming
* **Other SWD tools**: Via the Binary API (integration work may be required)
* **Custom software**: REST API for web applications, Binary API for high-performance applications

## Technical Questions

**Q: How fast is Airfrog's SWD communication?**

A: Airfrog supports four speed settings:
* **Turbo**: 4MHz (default)
* **Fast**: 2MHz  
* **Medium**: 1MHz
* **Slow**: 500KHz

Use slower speeds for long wires or noisy environments. Turbo works well for direct connections.

**Q: What's the difference between the REST API and Binary API?**

A: 

* **REST API**: Easy to use HTTP/JSON interface, great for web applications and testing
* **Binary API**: High-performance TCP protocol, minimal overhead, ideal for integrating with existing debugging tools

For example, a single read operation: REST API uses 200+ bytes, Binary API uses just 7 bytes total (not including TCP/IP headers), including both request and response.

**Q: Can Airfrog work alongside the target's normal operation?**

A: Yes! This is one of Airfrog's key features. It can:
* Read target memory and peripherals without halting execution
* Monitor counters and status in real-time
* Control peripherals in parallel with the target CPU
* Add networking capabilities without target CPU overhead

**Q: How does Airfrog boot so quickly?**

A: Airfrog transmits data from the target device within 5 seconds of power-on thanks to:
* Rust's fast boot times with no RTOS overhead
* Pre-configured WiFi credentials baked into firmware
* Efficient ESP32-C3 startup
* Parallel initialization of WiFi, HTTP and SWD communication

**Q: How does Airfrog implement concurrent operations?**

A: Using embassy-rs async framework:

* **SWD operations**: Single-threaded with async/await for non-blocking I/O
* **Network handling**: Multiple async tasks for web server
* **Target monitoring**: Background tasks for auto-connect and keepalive functionality
* **No RTOS overhead**: Direct ESP32-C3 HAL usage with cooperative multitasking

**Q: Why did you choose ESP32-C3?**

A: The ESP32-C3 was chosen for several reasons:

* **Performance**: Its high clock speed provide excellent performance for real-time applications.
* **Cost**: The ESP32-C3 is very affordable, making it suitable for low-cost applications.
* **Low Resource Usage**: It has a small footprint, allowing Airfrog to fit into a tiny PCB.
* **Rich Feature Set**: It includes built-in WiFi and Bluetooth, as well as a variety of peripherals, making it versatile for different use cases.
* **Community Support**: The ESP32 family has a large community and extensive documentation, which helps accelerate development.

**Q: Why did you choose Rust?**

A: Rust was chosen for several reasons:

* **Safety**: Rust's memory safety guarantees help prevent common bugs like null pointer dereferences and buffer overflows.
* **Performance**: Rust's zero-cost abstractions allow for high-performance code without sacrificing safety.
* **Concurrency**: Rust's ownership model makes it easier to write concurrent code without data races.
* **Embassy**: The embassy-rs async framework provides a lightweight, efficient way to handle asynchronous operations on embedded devices.
* **Ecosystem**: The Rust ecosystem provides powerful libraries for networking, async programming, and embedded development.
* **Fun**: Rust is a joy to work with.

As well as implementing the Airfrog functionality for its own sake, by writing Airfrog in Rust, we have increased the pool of high performance embedded applications that be used as examples to help future developers learn Rust programming for embedded systems.

**Q: How do you achieve 4MHz SWD, and why can't you go faster?**

A: Airfrog achieves 4MHz SWD by bitbanging using the ESP32-C3's high-speed GPIOs. 4MHz is the fastest rate that can be achieved reliably with the ESP32-C3's 160MHz clock while dealing with the requirements of the SWD protocol.  4MHz is an **approximate** rate.

To go faster would require either a fast processor (and likely large board) or a hardware implementation of SWD.  It may be possible to achieve higher speeds by using the ESP32-C3's SPI peripheral.

**Q: Does Airfrog impact the target device's performance?**

A: SWD is a completely parallel capability to the target device's primary core(s).  It has access to most of the same buses on the target device (flash, RAM, hardware peripherals), but its access it prioritised behind the primary core(s).  It is possible to cause bus contention using SWD, but at most this is likely limited to the odd 1-2 cycle CPU wait states.

As an example of its lack of impact, using Software Defined Retro ROM, with the MCU clocked at its minimum speed where it could still reliably serve ROMs, Airfrog accessing the device's flash and RAM did not cause any performance impact.

## Licensing and Commercial Use

**Q: Can I use Airfrog commercially?**

A: Yes! Airfrog is fully open source with permissive licensing:
* **Software**: MIT License - use commercially without restrictions
* **Hardware**: CC-BY-SA-4.0 - share improvements but free for commercial use
* **No closed source dependencies** - completely open stack

See [LICENCE](LICENCE) for details.

**Q: Can I modify the hardware design?**

A: Absolutely. The PCB designs are open source and you can:
* Modify the design for your specific needs
* Integrate Airfrog into your own products
* Manufacture and sell modified versions (following CC-BY-SA-4.0 terms)

One of the key modifications that might be desirable is to change the 5-pin connector to be more compatible with your target device.

## Troubleshooting

**Q: Airfrog isn't connecting to my target - what should I check?**

A: Common issues and solutions:

* **Check SWD support**: Verify your ARM device supports SWD protocol and it is not disabled by the target's firmware
* **Verify connections**: Ensure SWDIO, SWCLK, and ground are properly connected
* **Try slower speed**: Use `/api/target/config/speed` to set "Slow" or "Medium"
* **Check power**: Target device must be powered and running
* **Line length**: Keep SWD traces or cables as short as possible for reliable communication at high speeds

**Q: I'm getting parity errors - what's wrong?**

A: Parity errors typically indicate signal quality issues:

* **Reduce SWD speed**: Try "Medium" or "Slow" settings
* **Shorten connections**: Long or noisy SWD lines cause problems
* **Check power supply**: Ensure clean, stable power to both devices
* **Add ground plane**: Better PCB grounding can reduce noise

**Q: The target reports "Fault" acknowledge - what does this mean?**

A: A "Fault" acknowledge means you've attempted an invalid operation:

* **Wrong register access**: Check you're accessing valid DP/AP registers
* **Device state issue**: Target may be in a protected or sleep state
* **Clear errors**: Use `/api/target/clear-errors` or the ABORT register
* **Reset sequence**: Try `/api/target/reset` to reinitialize

**Q: Can I recover if Airfrog gets into a bad state?**

A: Yes, several recovery options:

* **Target reset**: Use `/api/target/reset` to reinitialize SWD communication
* **Line reset**: Use `/api/raw/reset` for low-level SWD reset
* **Power cycle**: Reset both Airfrog and target devices
* **Re-flash firmware**: Connect via USB serial and re-flash if needed

**Q: Does Airfrog have logging?**

A: Yes, Airfrog produces logs via its UART programming interface.  Logs are produced at different levels:

* **Error**: Critical issues that prevent operation
* **Warning**: Non-critical issues that are likely to affect performance
* **Info**: General operational messages
* **Debug**: Detailed information for troubleshooting
* **Trace**: Low-level operation details

You can enable different log levels via the `ESP_LOG` environment variable.  For example, to enable global Airfrog logging at the "Info" level, `ESP_LOG=info` before running the firmware:

```bash
ESP_LOG=info SSID=your-ssid PASSWORD=your-password cargo run -p airfrog
```

To limit logs to airfrog only, use `ESP_LOG=airfrog=info`.
