# REST API

This document describes the airfrog's REST API for programming and co-processing with SWD targets.

## API Summary

### Target Control

```text
GET   /api/target/status                      # Get connection status and basic target info
POST  /api/target/reset                       # Reset/initialize target for debugging  
GET   /api/target/details                     # Get detailed target information
GET   /api/target/errors                      # Get current SWD error states
POST  /api/target/clear-errors                # Clear SWD error states
```

### SWD Configuration

```text 
GET   /api/config/swd/runtime/speed                       # Get current SWD communication speed
POST  /api/config/swd/runtime/speed                       # Set SWD communication speed
POST  /api/config/swd/runtime                             # Update runtime SWD settings
POST  /api/config/swd/flash                               # Update stored SWD settings
```

### Airfrog Network Configuration
```text
GET   /api/config/net/runtime                             # Get current network configuration
POST  /api/config/net/runtime                             # Update runtime network settings
```

### Airfrog Control
```text
POST  /api/reboot                                         # Reboot the Airfrog device   
```

### Memory Operations

```text
GET   /api/target/memory/read/0x{addr}        # Read single 32-bit word from memory
POST  /api/target/memory/write/0x{addr}       # Write single 32-bit word to memory
POST  /api/target/memory/bulk/read/0x{addr}   # Read multiple words from consecutive addresses
POST  /api/target/memory/bulk/write/0x{addr}  # Write multiple words to consecutive addresses
```

## Flash Operations

```text
POST  /api/target/flash/unlock                # Unlock flash for programming
POST  /api/target/flash/lock                  # Lock flash to prevent programming
POST  /api/target/flash/erase-sector/{sector} # Erase specific flash sector
POST  /api/target/flash/erase-all             # Erase entire flash memory
POST  /api/target/flash/write/0x{addr}        # Write single word to flash
POST  /api/target/flash/bulk/write/0x{addr}   # Write multiple words to flash
```

## Raw Register Operations

```text
POST  /api/raw/reset                                  # Perform JTAG to SWD reset
GET   /api/raw/dp/read/0x{register}                   # Read raw DP register
POST  /api/raw/dp/write/0x{register}                  # Write raw DP register
GET   /api/raw/ap/read/0x{ap_index}/0x{register}      # Read raw AP register
POST  /api/raw/ap/write/0x{ap_index}/0x{register}     # Write raw AP register
POST  /api/raw/ap/bulk/read/0x{ap_index}/0x{register} # Read multiple raw AP registers
POST  /api/raw/ap/bulk/write/0x{ap_index}/0x{register}# Write multiple raw AP registers
POST  /api/raw/clock                                  # Clock SWD interface with specified SWDIO state
```

## Usage Notes

1. **Target must be reset/initialized** before most operations can be performed
2. **Flash operations require unlocking** flash memory first
3. **Bulk operations are limited** to 256 words (1KB) maximum
4. **All addresses and data** ap_index and registers are represented as hex strings (sector is not)
5. **Flash programming** requires erased target locations (0xFF)
6. **Writes must be aligned** to 32-bit boundaries
7. **Raw register operations** provide low level SWD access and usage can break the other APIs - use either the standard APIs **or** raw.

## Limits

- **Addresses**: 32-bit hex values (e.g., `"0x20000000"`)
- **Data**: 32-bit hex values (e.g., `"0x12345678"`)
- **Bulk Operations**: Maximum 4096 words (16KB) per operation

## Base URL
All endpoints are relative to: `http://<airfrog-ip>/api`

## Common Response Format

Uses HTTP status codes to indicate success/failure:
- **200 OK** - Operation successful
- **400 Bad Request** - Invalid parameters or malformed JSON  
- **500 Internal Server Error** - SWD/target operation failed
- **503 Service Unavailable** - Target not connected/initialized

### Target Control Operations

These additionally include `status` field with SWD connection information:

```json
// GET /api/target/status
// POST /api/target/reset
// GET /api/target/details  
{
  "status": {
    "connected": true,
    "version": "V1",
    "idcode": "0x2BA01477",
    "mcu": "STM32F411",
    "settings": {
      "speed": "Turbo",
      "auto_connect": true,
      "keepalive": true,
      "refresh": true
    }
  }
}
```

### Memory/Flash/Config Operations

Minimal responses are provided:

```json
// Success with data
{ "data": "0x12345678" }
```

```json
// Success with no data
{}
```

```json
// Success with array data
{ "data": ["0x12345678", "0x9ABCDEF0"] }
```

### Error Responses

```json
{"error": {"swd": {"kind": "bad acknowledge", "detail": "7"}}}
```

## Error Messages

There are two types of error messages:
- **swd** - These originate in airfrog's SWD stack, or on the SWD target itself.
- **airfrog** - These originate airfrog's API logic, and typically indicate problems with the API usage or request formatting.

| Type | Kind | Description |
|------|------|-------------|
| swd | `wait acknowledge` | SWD target returned WAIT response - max internal retries exceeded |
| swd | `bad acknowledge` | Invalid ACK response from target - usually means target in a bad state |
| swd | `fault acknowledge` | Target returned FAULT response - usually means invalid operation attempted |
| swd | `read parity error` | Parity error during read operation - consider slowing the speed |
| swd | `debug port error` | The target's debug port reported an error - usually means an invalid operation was attempted |
| swd | `operation failed` | Operation failure - likely in higher level logic above SWD protocol |
| swd | `not ready` | Target not initialized or powered up |
| swd | `api error` | Invalid parameters or API usage |
| swd | `unsupported operation` | Operation not supported for this target |
| airfrog | `bad request` | Invalid API request - malformed JSON or missing parameters |
| airfrog | `invalid body` | Request body is not valid JSON or missing required fields |
| airfrog | `invalid path` | Requested API path does not exist or is malformed |
| airfrog | `invalid method` | HTTP method not allowed for this endpoint |
| airfrog | `timeout` | Currently unused |
| airfrog | `too large` | Requested to read or write too many words of bulk data |
| airfrog | `internal server error` | Unexpected error occurred in airfrog's API logic |

Further error messages may be added as needed to provide more context on failures. 

---

# API

## Target Control

### Get Target Status
```
GET /api/target/status
```

Returns current connection status and basic target information.  Note this command **succeeds** even if there is no target connected.  [Reset Target](#reset-target) fails if no target is connected, so may be a better choice if you are looking for an error response in this scenario.

`auto` field indicates whether auto-connect is enabled for this target.  It is enabled by default (on power on), and disabled via `/api/raw/reset`.  It is re-enabled by `/api/target/reset`.

`keepalive` field indicates whether keepalive is enabled for this target.  It is enabled by default (on power on), and disabled via `/api/raw/reset`.  It is re-enabled by `/api/target/reset`.

**Response:**
```json
{
  "status": {
    "connected": true,
    "idcode": "0x2BA01477", 
    "mcu": "STM32F411",
    "settings": {
      "speed": "Turbo",
      "auto_connect": true,
      "keepalive": true,
      "refresh": true
    }
  }
}
```

### Reset Target
```
POST /api/target/reset
```

Performs SWD reset sequence and initializes the target for debugging.  Also (re-)enables auto-connect and keepalives for the target.

**Response:**
```json
{
  "status": { "connected": true, "idcode": "0x2BA01477", "mcu": "STM32F411" }
}
```

### Get Target Details
```
GET /api/target/details
```

Returns detailed information about the connected target.

**Response:**
```json
{
  "status": { "connected": true, "idcode": "0x2BA01477", "mcu": "STM32F411" },
  "data": {
    "idcode": "0x2BA01477",
    "mcu_family": "STM32F4",
    "mcu_line": "STM32F411",
    "mcu_device_id": "0x431",
    "mcu_revision": "A/1/Z",
    "flash_size_kb": 512,
    "unique_id": "0x123456789ABCDEF011223344",
    "mem_ap_idr": "0x24770011"
  }
}
```

### Retrieve SWD Errors

```
GET /api/target/errors
```

Returns current error states from the Debug Port without clearing them.

**Response:**
```json
{
  "status": { "connected": true, "idcode": "0x2BA01477", "mcu": "STM32F411" },
  "data": {
    "stkerr": false,
    "stkcmp": true, 
    "wderr": false,
    "orunerr": false,
    "readok": true
  }
}
```

### Clear SWD Errors

```
POST /api/target/clear-errors
```

Clears any error states in the Debug Port (STKERR, STKCMP, WDERR, ORUNERR).

**Response:**
```json
{
  "status": { "connected": true, "idcode": "0x2BA01477", "mcu": "STM32F411" }
}
```

---

## Target Configuration

### Get SWD Speed
```
GET /api/config/swd/runtime/speed
```

Returns the current SWD communication speed.

**Response:**
```json
{
  "speed": "Turbo"
}
```

### Set SWD Speed
```
POST /api/config/swd/runtime/speed
```

Sets the SWD communication speed.

**Request Body:**
```json
{
  "speed": "Turbo"
}
```

Valid speeds: `"Slow"`, `"Medium"`, `"Fast"`, `"Turbo"`

**Response:**
```json
{}
```

### Set Runtime Config

```
POST /api/config/swd/runtime
```

Updates the Airfrog runtime configuration.  All settings must be supplied.

**Request Body:**
```json
{
  "speed": "Turbo",
  "auto_connect": true,
  "keepalive": true,
  "refresh": true
}
```


**Response:**
```json
{}
```

### Set Runtime Config

```
POST /api/config/swd/flash
```

Updates the Airfrog configuration stored to flash.  Is only applied to a running system after reboot.  All settings must be supplied.

**Request Body:**
```json
{
  "speed": "Turbo",
  "auto_connect": true,
  "keepalive": true,
  "refresh": true
}
```


**Response:**
```json
{}
```

---

## Aifrog Network Configuration

### Get Network Config

```
GET /api/config/net/runtime
```

**Response:**
```json
{
  "ap_password": "airfrog",
  "ap_ssid": "airfrog",
  "ap_v4_ip": [192, 168, 4, 1],
  "ap_v4_netmask": [255, 255, 255, 0],
  "mode": "StaFallbackToAp",
  "sta_password": "sta-ssid",
  "sta_ssid": "sta-password",
  "sta_v4_dhcp": true,
  "sta_v4_dns0": [255, 255, 255, 255],
  "sta_v4_dns1": [255, 255, 255, 255],
  "sta_v4_gw": [255, 255, 255, 255],
  "sta_v4_ip": [255, 255, 255, 255],
  "sta_v4_netmask": [255, 255, 255, 255],
  "sta_v4_ntp": [255, 255, 255, 255]
}
```

### Update Network Config

```
POST /api/config/net/runtime
```

Updates the Airfrog network configuration.  All settings must be supplied.

**Request Body:**
```json
{
  "ap_password": "airfrog",
  "ap_ssid": "airfrog",
  "ap_v4_ip": [192, 168, 4, 1],
  "ap_v4_netmask": [255, 255, 255, 0],
  "mode": "StaFallbackToAp",
  "sta_password": "sta-ssid",
  "sta_ssid": "sta-password",
  "sta_v4_dhcp": true,
  "sta_v4_dns0": [8, 8, 8, 8],
  "sta_v4_dns1": [8, 8, 4, 4],
  "sta_v4_gw": [192, 168, 1, 1],
  "sta_v4_ip": [192, 168, 1, 100],
  "sta_v4_netmask": [255, 255, 255, 0],
  "sta_v4_ntp": [192, 168, 1, 1]
}
```

**Response:**
```json
{}
```

## Airfrog Control

### Reboot Airfrog Device

```
POST /api/reboot
```

Reboots the Airfrog device.  This is a hard reset and will disconnect any active connections.

**Request Body:**
```json
{}
```

**Response:**
```json
{}
```

## Memory Operations

### Read Single Word

```
GET /api/target/memory/read/0x{addr}
```

Reads a 32-bit word from the specified address.

**Example:**

```
GET /api/target/memory/read/0x20000000
```

**Response:**

```json
{
  "data": "0x12345678"
}
```

### Write Single Word

```
POST /api/target/memory/write/0x{addr}
```

Writes a 32-bit word to the specified address.

**Request Body:**
```json
{
  "data": "0x12345678"
}
```

**Response:**
```json
{}
```

### Read Multiple Words
```
POST /api/target/memory/bulk/read/0x{addr}
```

Reads multiple consecutive 32-bit words starting from the specified address.

**Request Body:**
```json
{
  "count": 256
}
```

**Response:**
```json
{
  "data": ["0x12345678", "0x9ABCDEF0", "0x11223344", "..."]
}
```

### Write Multiple Words
```
POST /api/target/memory/bulk/write/0x{addr}
```

Writes multiple consecutive 32-bit words starting from the specified address.

**Request Body:**
```json
{
  "data": ["0x12345678", "0x9ABCDEF0", "0x11223344"]
}
```

**Response:**
```json
{}
```

---

## Flash Operations

### Unlock Flash
```
POST /api/target/flash/unlock
```

Unlocks the target's flash memory for programming operations.

**Response:**
```json
{}
```

### Lock Flash
```
POST /api/target/flash/lock
```

Locks the target's flash memory, preventing further programming.

**Response:**
```json
{}
```

### Erase Flash Sector
```
POST /api/target/flash/erase-sector/{sector}
```

Erases a specific flash sector.

**Response:**
```json
{}
```

### Erase All Flash
```
POST /api/target/flash/erase-all
```

Erases all flash sectors on the target device.

**Response:**
```json
{}
```

### Write Flash Word
```
POST /api/target/flash/write/0x{addr}
```

Writes a single 32-bit word to flash memory.

**Request Body:**
```json
{
  "data": "0x12345678"
}
```

**Response:**
```json
{}
```

### Write Flash Bulk
```
POST /api/target/flash/bulk/write/0x{addr}
```

Writes multiple consecutive 32-bit words to flash memory.

**Request Body:**
```json
{
  "data": ["0x12345678", "0x9ABCDEF0", "0x11223344"]
}
```

**Response:**
```json
{}
```

---

## Raw Register Operations

### JTAG to SWD Reset

Disables auto-connect and keepalive, and performs a JTAG to SWD reset sequence.  Auto-connects and keepalives are re-enabled by `/api/target/reset`.

```
POST /api/raw/reset
```

**Response:**
```json
{}
```

### Read DP Register
```
GET /api/raw/dp/read/0x{register}
```

Reads a raw Debug Port register.

**Example:**
```
GET /api/raw/dp/read/0x0
```

**Response:**
```json
{
  "data": "0x2BA01477"
}
```

### Write DP Register
```
POST /api/raw/dp/write/0x{register}
```

Writes a raw Debug Port register.

**Request Body:**
```json
{
  "data": "0x50000000"
}
```

**Response:**
```json
{}
```

### Read AP Register
```
GET /api/raw/ap/read/0x{ap_index}/0x{register}
```

Reads a raw Access Port register. Automatically selects the specified AP via DP SELECT register.

**Example:**
```
GET /api/raw/ap/read/0x0/0xC
```

**Response:**
```json
{
  "data": "0x24770011"
}
```

### Write AP Register
```
POST /api/raw/ap/write/0x{ap_index}/0x{register}
```

Writes a raw Access Port register. Automatically selects the specified AP via DP SELECT register.

**Request Body:**
```json
{
  "data": "0x23000052"
}
```

**Response:**
```json
{}
```

### Bulk Read AP Registers

```
POST /api/raw/ap/bulk/read/0x{ap_index}/0x{register}
```

**Request Body:**
```json
{
  "count": 256
}
```

**Response:**
```json
{
  "data": ["0x24770011", "0x23000052", "0x12345678", "..."]
}
```

### Bulk Write AP Registers

```
POST /api/raw/ap/bulk/write/0x{ap_index}/0x{register}
```

**Request Body:**
```json
{
  "count": 256,
  "data": ["0x24770011", "0x23000052", "0x12345678", "..."]
}
```

### Clock SWD Interface

```
POST /api/raw/clock
```

**Request Body:**
```json
{
  "level": "Low",
  "post": "Input",
  "count": 8
}
```

Args:
- `level` - The state of the SWDIO line during the clock operation.
- `post` - The state of the SWDIO line after the clock operation is complete.
- `count` - The number of clock cycles to perform.

`level` and `post` can be one of:
- `"Input"` - SWDIO line set to input state
- `"Low"` - SWDIO line set to low state
- `"High"` - SWDIO line set to high state

**Response:**
```json
{}
```

---
