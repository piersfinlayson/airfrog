# Software Integration

Airfrog is, amongst other things, capable of being a standard SWD debug probe, for debugging and programming ARM microcontrollers.  Rather than being wired, like most SWD probes, airfrog connects over WiFi, allowing you to debug and program your targets wirelessly.  This can be helpful when physical proximity to the target is inconvenient, either due to access, high voltages, etc.  Therefore it is possible to use airfrog with existing software tools that support SWD debugging and programming.

## probe-rs

Airfrog is pre-integrated with the excellent [probe-rs](https://probe.rs/) tool out of the box.  As of writing, you need to use a [fork of probe-rs](https://github.com/piersfinlayson/probe-rs) that has support for airfrog.  This fork is not yet merged into the mainline, but it is expected to be offered upstream in the future.

To use this version of probe-rs:

1. Clone the forked repository:
   ```bash
   git clone https://github.com/piersfinlayson/probe-rs.git
   ```

2. Change into the directory and build `probe-rs`:
   ```bash
   cd probe-rs
   cargo build --release
   ```

3. Add the `target/release` directory to your `PATH`:
   ```bash
    export PATH="$PATH:$(pwd)/target/release"
    ```

4. Use `probe-rs` - for example to program firmware.  Replace `<airfrog-ip>` with the IP address of your airfrog device and `<firmware>` with the path to your target's firmware:
    ```bash
    probe-rs --probe 0000:0000:airfrog:<airfrog-ip>:4146 --chip STM32F411RETx <firmware.elf>
    ```

Using this example, probe-rs will connect to the airfrog device, wirelessly, program the specified firmware, and attach to RTT logging.

Note the format of an Airprog probe: `0000:0000:airfrog:<airfrog-ip>:4146`.  It is currently not possible to detect and airfrog probe using `probe-rs list`.

## Other Tools

Integration should be feasible with other platforms, using airfrog's [Binary API](./BINARY-API.md).  Other tools that support SWD debugging and programming, and for which integration may be interesting, include:
- [OpenOCD](http://openocd.org/)
- [pyOCD](https://pyocd.io/)
