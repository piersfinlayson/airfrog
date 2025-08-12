# SWD Protocol

Contents:

- [Overview](#overview)
- [Operation Byte](#operation-byte)
- [Turnaround Bit](#turnaround-bit)
- [Additional Clock Cycles](#additional-clock-cycles)
- [Acknowledge](#acknowledge)
- [Parity](#parity)
- [Debug and Access Ports](#debug-and-access-ports)
- [Line Reset](#line-reset)
- [DP Registers](#dp-registers)
- [AP Registers](#ap-registers)
- [V2 and Multi-Drop](#v2-and-multi-drop)

## Overview

SWD consists of two actors:
- **Host**: The device controlling the SWD communication, typically a debugger or programmer - airfrog in this case.
- **Target**: The device being debugged or programmed, typically a microcontroller - such an STM32 or Raspberry Pi Pico.

SWD is as it suggests a serial protocol, using two lines:
- **SWDIO**: Serial data line
- **SWCLK**: Serial clock line

Ground must also be commoned between the host and target devices.

Once SWD communication has been established (see [Line Reset](#line-reset)), the host and target communicate by sending and receiving packets over the data line, synchronised by the clock line.

The host is always in control of the clock line, and initiates all communication.  The target and host communicate by sending and receiving packets over the SWDIO line, which is bidirectional.

- 1s are sent with a high level on SWDIO, and a single clock pulse on SWCLK.
- 0s are sent with a low level on SWDIO, and a single clock pulse on SWCLK.

The target reads the SWDIO line on the rising edge of the clock.  The host reads the SWDIO line just before it sends the clock high.

All communication apart from the initial, reset, sequence, consists of:
- An [**operation**](#operation) byte.
- A [**turnaround**](#turnaround-bit) bit, where the host releasing the SWDIO line, and send a single clock pulse.  This allows the target to prepare to send data.
- A 3 bit [**acknowledge**](#acknowledge), where the target indicates whether it can process the operation.
- If the operation is a write, or the acknowledge indicated an error, another [**turnaround**](#turnaround) bit, where the host sets SWDIO as output, and sends a single clock pulse.
    - If the acknowledge indicated an error, there is no furthe transmission on this operation.
- A 33-bit data sequence where either the host or target (write or read operation respectively) sends 32-bits of data, followed by a parity bit.
- If the operation was a read, a final [**turnaround**](#turnaround) bit, where the host sets SWDIO as output, and sends a single clock pulse.

At the end of the sequence, the host is always in control of the SWDIO line, by virtue of the turnaround bits.

All transmission is LSB first.

## Operation

| Bit    | Name      | Description                                                      |
|--------|-----------|------------------------------------------------------------------|
| Bit 0  | **Start** | Always 1                                                         |
| Bit 1  | **APnDP** | 0 for DP, 1 for AP (selects Debug Port or Access Port)           |
| Bit 2  | **RnW**   | 0 for write, 1 for read                                          |
| Bit 3  | **A[2]**  | 2nd bit of DP/AP register address to read/write                  |
| Bit 4  | **A[3]**  | 3rd bit of DP/AP register address to read/write                  |
| Bit 5  | [**Parity**](#parity)| Parity bit, calculated on bits 1-4                               |
| Bit 6  | **Stop**  | Always 0                                                         |
| Bit 7  | **Park**  | Always 1                                                         |

## Turnaround

Turnaround bits are used to allow the controller of SWDIO to be switched.  The number of turnaround bits are configurable in some SWD implementations.

## Additional Clock Cycles

The target SWD implementation requires some additional clock cycles in certain circumstances, beyond the **turnaround** bits:
- If the host is not planning to send any more data (perform any further operations), an additional 8 clock cycles (with SWDIO low) are sent after the final bit.  This is to allow the target to clock the operation through the target's debug port.
- The STM32F4 reference manual states that a write must be followed two additional clock cycles after the final parity bit with SWDIO low.  This only needs to be done if the host is not planning to send any more data, as otherwise the host will continue to clock.

## Acknowledge

There are three valid values for the acknowledge:
- 0b001 - **OK**: The target can process the operation.
- 0b010 - **Wait**: The target is not ready to process the operation.
- 0b100 - **Fault**: The target cannot process the operation due to an error.

Note that as the acknowledge is, like all data in SWD, LSB first, a 100 on the wire indicates **OK**, and 001 **Fault**.

In the real world:
| Acknowledge | Meaning                | Typical Cause / Action                                                                                      |
|-------------|------------------------|------------------------------------------------------------------------------------------------------------|
| **OK**      | Well Done              | You are behaving properly, querying AP/DP registers and targeting valid MCU addresses.                      |
| **Wait**    | Retry                  | Likely bus contention or the target is busy (e.g., AHB bus). Retry the operation.                          |
| **Fault**   | You Screwed Up | Invalid register access, inaccessible due to device state, or protocol error. Investigate and correct issue.|

## Parity

Parity is always calculated with an even number of 1 bits being 0,  and an off number being 1.
- The parity bit on an operation byte covers bits 1-4 inclusive, so a mask of 0x1E.
- The parity bit on a data byte covers all 32 bits preceeding the parity bit. 

Parity errors tend to occur when the clock is of poor quality, too fast, or there is too much noise on the SWD lines.

If the host sees a parity error, the target will not know.  All you can do is retry, possibly slowing down.

If the target sees a parity error, you will likely get a **Fault** acknowledge, and the target will not process the operation.  Once you receive a **Fault** acknowledge, you need to figure out the problem, clear the fault on the target (via the ABORT register), and then retry with a rectified operation, slower clock, etc.

## Debug and Access Ports

The Debug Port provides overall control and status of the target debug capability.

The Access Port provides access to various aspects of the target device, such as memory, registers, and flash.

Both the DP and AP consist of a set of registers that can be read and written to, selecting DP or AP by using the **APnDP** bit in the operation byte.

A DP/AP register always has bits 0-1 low, hence only signaling the 2nd and 3rd bits in the operation byte.  When accessing registers above 0x0C, bits 4-7 are pre-set in the DP SELECT register, and the operation byte only uses bits 2-3.

DP registers typically differ between read and write operations for the same address.  For example, register 0x00 is DP IDCODE when read, and ABORT (to clear errors) when written to.

AP registers are typically the same for read and write operations, although may have slightly different meanings.  For example the DRW (Data Read/Write) register, 0x0C either reads a word of data from the target, or writes a word of data to the target (using the TAR Target Address Register to select the address).

Again, think of the DP as the overall control and status of the target debug capability, and the AP as the access to the target device's memory, registers, and flash.

## Line Reset

To start communication with the target, the host must perform a line reset, and instruct the target to enter SWD debug mode (instead of JTAG).

This is done by:
- Pulling SWDIO high for at least 50 clock cycles.
- Sending a 16-bit JTAG to SWD sequence.
- Pulling SWDIO high for at least 50 clock cycles again.
- Pulling SWDIO low for at least 2 clock cycles.

The host must then also read the DP IDCODE register (0x00).  If the host attempts to perform any other SWD operation before reading the DP IDCODE register, the target will respond with a **Fault** acknowledge.

The JTAG to SWD sequence is `0b0111100111100111`, `0x79E7`.  However, it is sent LSB first, so would typically be considered to be `0xE79E`.

After an error, it is a good idea to perform a line reset, as the target may be in an unknown state.  This is particularly true after receiving a **Fault** acknowledge.

## DP Registers

The primary ones are listed below.

Note that there are different versions of the Debug Port (v0, v1, and v2), and the registers may differ slightly between versions. 

### Read

| Register   | Address | Description                                                                                                                                         |
|------------|---------|-----------------------------------------------------------------------------------------------------------------------------------------------------|
| **DPIDR**  | 0x00    | Debug Port Identification Register. Provides the IDCODE of the target ARM device (not the MCU). Example: Cortex-M4 with FPU r0p1 has 0x2BA01477.    |
| **CTRL/STAT** | 0x04 | Control and Status Register. Used to control the debug port and read its status. After a **Fault** acknowledge, read this to diagnose the problem.  |
| **RDBUFF** | 0x0C    | Read Buffer Register. Holds the result of the last AP read operation. The actual AP value is read from RDBUFF after initiating an AP read.           |

### Write

| Register    | Address | Description                                                                                                         |
|-------------|---------|---------------------------------------------------------------------------------------------------------------------|
| **ABORT**   | 0x00    | Used to clear any errors in the debug port and reset its state. Typically used after receiving a **Fault** acknowledge. |
| **CTRL/STAT** | 0x04  | Control and Status Register. See above for details.                                                                 |
| **SELECT**  | 0x08    | Used to set high bits (4-7) of the AP/DP register address and select different AP instances.                        |

## AP Registers

The AP register depend on the specific type of AP being used (selected using the select bits on the DP SELECT register).  This is typically, by default, a MEM-AP (Memory Access Port), which provides access to the target's memory and registers.

Using the MEM-AP, any valid address in the target's memory space, including flash, hardware registers and chip ROM can be accessed.

Flash can be erased and written, by accessing the device's flash controller registers, and then writing to the flash memory itself.

Hardware registers can be read and written, allowing control of the target device's peripherals.

### Generic AP Registers

All APs expose a read-only register:

| Register | Address | Description                                                                                                                        |
|----------|---------|------------------------------------------------------------------------------------------------------------------------------------|
| **IDR**  | 0xFC    | Access Port Identification Register. Provides the IDCODE of the target AP device, identifying the type of AP (e.g., MEM-AP, JTAG-AP). An STM32F4 MEM-AP has an IDR value of 0x24770011. |

Again, the primary ones are listed below.

### MEM-AP Registers

| Register | Address | Description |
|----------|---------|-------------|
| **CSW**  | 0x00    | Control and Status Word. Configures the MEM-AP, including access size and auto-increment settings. |
| **TAR**  | 0x04    | Target Address Register. Specifies the address for memory or register read/write operations. |
| **DRW**  | 0x0C    | Data Read/Write. Transfers data to/from the target's memory or registers. On reads, the first value must be discarded; subsequent reads return the previous value, with the final value available in the RDBUFF register. |

## Problems

SWD is a simple and fairly robust protocol.  Key problems that have been seen during development are:

* If the SWDIO line is left high when clocking additional clock cycles (which are required by various parts of the protocol), the target interprets this as a command start bit.  This leaves the probe and target in mis-matched states.  Care must be taken that additional clock cycles are always sent with SWDIO low.  (This does not apply for turnaround bits, which are always sent with SWDIO released by aifrog.)

* Parity errors tend to occur when the SWD lines are long and noisy.  See [Parity](#parity) for more details.

* Problems have been observed communicating with the SWD target in STM32F4s whe the core is halted.  This was tracked down to noisy SWD lines.  When airfrog was directly connected to the target (traces totalling < 1" in length), the problem did not occur.

See also:

* [Acknowledge](#acknowledge) for more details on the acknowledge values, which are used by the target to report error states.

* CTRL/STAT and ABORT [DP Registers](#dp-registers) for more details on how to read and clear errors in the debug port.

## V2 and Multi-Drop

V2 is a newer version of the SWD protocol, which has two key differences from V1:

- A different reset sequence is used - which takes the SWD out of "dormant state".
- Multi-drop support is added, alongside existing single-drop support.

Both RP2040 (Pico) and RP2350 (Pico 2) use V2 SWD.

- The RP2040 requires the use of multi-drop SWD - as it has a DP for each of core 0, core 1, and a rescue DP.

- The RP2350 has a single DP, and does not **require** the use of multi-drop (meaning TARGETSEL does not need to be used - see [Multi-Drop](#multi-drop) below) but does **support** multi-drop SWD.  This allows a debugger to be connected to multiple RP2350s simultaneously.

### Dormant State

The reset sequence for V2 SWD looks a bit different to V1, and is as follows:

- Pull SWDIO high for at least 50 clock cycles (like v1)
- Send the JTAG to dormant sequence - 31 bits: 0x33BBBBBA.  This is optional, required if the target starts out in JTAG mode.
- Pull SWDIO high for at least 8 clock cycles.
- Send the selection alert sequence, which is 128 bits: [0x6209F392, 0x86852D95, 0xE3DDAFE9, 0x19BC0EA2]
- Pull SWDIO low for 4 clock cycles.
- Send the SWD activation code, 8 bits 0x1A.
- Perform a traditional (v1) reset sequence of pulling SWDIO high for at least 50 clock cycles and then pulling SWDIO low for at least 2 clock cycles.

It can also be useful to put the target into dormant state, for example, prior to performing initialization.  This is done as follows:

- Pull SWDIO high for at least 50 clock cycles.
- Send the SWD to dormant sequence - 16 bits: 0xE3BC.

### Multi-Drop

A multi-drop SWD target has multiple different debug ports (DP).  Instead of activating the single target following the reset sequence by reading the SWD target's IDCODE using the DPIDR register, the host must first select the appropriate target, by writing its 32-bit ID to the TARGETSEL register (0x0C).

Once the target is selected, the host reads the IDCODE from the DPIDR register and continues with the normal SWD protocol.

If a target is already selected, it should be de-selected first by writing 0xFFFFFFFF to the TARGETSEL register.

There is no automatic detection of targets - the host needs to know the ID of the target it wants to communicate with.  The RP2040 has three:
- 0x01002927 (core 0)
- 0x21002928 (core 1)
- 0xF1002929 (rescue DP)

To complicate matters, the top 4 bits of the core 0 and core 1 IDs can be modified by a hardware register.  Tis allows multiple RP2040s to be connected to the same SWD bus, and each core to have a unique ID.  The rescue DP does not have this feature, and always has the ID 0xF1002929.

While the RP2350 does not require the use of multi-drop and TARGETSEL, it is implementing it under the covers.  Hence if you write 0xFFFFFFFF to TARGETSEL, it will de-select the RP2350's only DP, and you will need to re-select it by a reset.
