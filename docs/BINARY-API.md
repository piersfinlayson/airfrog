# Binary API

High-performance binary protocol for using airfrog to program and debug SWD targets over WiFi. Typically used to integrate with existing programming and debugging tools.

For an easier to use, lower performance API, see the [REST API](./API.md).

## Connection Details

Airfrog acts as the server.  Clients connect to airfrog over TCP/IP.

**Protocol:** TCP  
**Default port:** `4146`

## Connection Lifecycle

1. **Connect:** TCP handshake + version negotiation
2. **Operations:** Client sends commands, receives responses
3. **Disconnect:** Client sends `0xFF` command or closes TCP connection
4. **airfrog response:** airfrog sends `0x00` ack and closes connection

## Connection Handshake

1. TCP connection established
2. **airfrog → client:** `[version:1]` (currently `0x01`)
3. **client → airfrog:** `[version:1]` (echo back `0x01` to acknowledge)
4. Normal protocol operations begin

If version mismatch or invalid ack is detected, connection should be closed by the client.  Airfrog similarly closes.

## Byte Order

**IMPORTANT:** All multi-byte fields use **little-endian** byte order throughout the protocol.

## Command Codes

| Code   | Operation        | Type   |
|--------|------------------|--------|
| `0x00` | DP Read          | Single |
| `0x01` | DP Write         | Single |
| `0x02` | AP Read          | Single |
| `0x03` | AP Write         | Single |
| `0x12` | AP Bulk Read     | Bulk   |
| `0x13` | AP Bulk Write    | Bulk   |
| `0x14` | Multi-reg Write  | Bulk   |
| `0xF0` | Ping             | Control|
| `0xF1` | Reset Target     | Control|
| `0xF2` | Clock            | Control|
| `0xF3` | Set Speed        | Control|
| `0xFF` | Disconnect       | Control|

## Request Formats

### Single Word Operations
```
DP Read:        [0x00:1][reg:1]                     # 2 bytes
DP Write:       [0x01:1][reg:1][data:4]             # 6 bytes
AP Read:        [0x02:1][reg:1]                     # 2 bytes
AP Write:       [0x03:1][reg:1][data:4]             # 6 bytes
```

### Bulk Operations
```
AP Bulk Read:    [0x12:1][reg:1][count:2]            # 4 bytes
AP Bulk Write:   [0x13:1][reg:1][count:2][data...]   # 4+4N bytes
Multi-reg Write: [0x14:1][count:2][ap_dp:1][reg:1][data:4]..  # 3+6N bytes
```

Multi-reg Write allows writing multiple registers in a single command, where `count` is the number of registers to write, followed by sets of

* one byte indicating 0x00 for DP or 0x01 for AP
* one byte for the register address
* four bytes for the 32-bit data value.

### Control Operations
```
Ping:           [0xF0:1]                            # 1 byte
Reset Target:   [0xF1:1]                            # 1 byte
Clock:          [0xF2:1][level|post<<4:1][cycles:2] # 4 bytes
Set Speed:      [0xF3:1][speed]                     # 2 bytes
Disconnect:     [0xFF:1]                            # 1 byte
```

See [Fields](#fields) for details on each field.

**Reset Target** tries a V1 reset.  If that fails (IDCODE read fails), it tries a V2 reset.  Multi-drop reset is not supported. by this command.

## Response Formats

### Single Word Operations

```
Success:        [0x00:1][data:4]                   # 5 bytes
Error:          [0x8X:1]                           # 1 byte
```

### Bulk AP Read Operation

```
Success:        [0x00:1][count:2][data...]         # 3+4N bytes
Error:          [0x8X:1]                           # 1 byte
```

Note that bytes may be returned on error, if some words were read.

### Bulk AP Write Operations

```
Success:        [0x00:1]                           # 1 bytes
Error:          [0x8X:1]                           # 1 byte
```

### Multi-reg Write Operations

```
Success:        [0x00:1]                           # 1 bytes
Error:          [0x8X:1]                           # 1 byte
```

### Control Operations
```
Ping:           [0x00:1]                           # 1 byte
Reset Target:   [0x00:1]                           # 1 byte (success)
Clock:          [0x00:1]                           # 1 byte (success)
Set Speed:      [0x00:1]                           # 1 byte (success)
Disconnect:     [0x00:1]                           # 1 byte
```

## Error Handling

- **Error Status:** Top bit set (`0x80 | error_code`)

### Error Codes

| Code | Description           | Usage |
|------|-----------------------|-------|
| `0x81` | Invalid command       | Unknown command byte |
| `0x82` | Register access error | SWD/target communication failure |
| `0x83` | Timeout               | Operation timeout |
| `0x84` | Connection error      | Network/connection issue |
| `0x85` | Invalid parameter     | Bad register address or count |

## Complete Protocol Reference

| Operation | Request Format | Success Response | Error Response |
|-----------|----------------|------------------|----------------|
| DP Read | `[0x00][reg]` | `[0x00][data:4]` | `[0x8X]` |
| DP Write | `[0x01][reg][data:4]` | `[0x00]` | `[0x8X]` |
| AP Read | `[0x02][reg]` | `[0x00][data:4]` | `[0x8X]` |
| AP Write | `[0x03][reg][data:4]` | `[0x00]` | `[0x8X]` |
| AP Bulk Read | `[0x12][reg][count:2]` | `[0x00][count:2][data...]` | `[0x8X][count:2][data...]` |
| AP Bulk Write | `[0x13][reg][count:2][data...]` | `[0x00][count:2]` | `[0x8X][count:2]` |
| Ping | `[0xF0]` | `[0x00]` | `[0x8X]` |
| Reset Target | `[0xF1]` | `[0x00]` | `[0x8X]` |
| Clock | `[0xF2][level\|post<<4:1][count:2]` | `[0x00]` | `[0x8X]` |
| Disconnect | `[0xFF]` | `[0x00]` | `[0x8X]` |

### Fields

- `reg`: Register address (8 bits)
- `count`(bulk): Number of 32-bit words (u16) - max 256 (1024 bytes)
- `count` (clock): Number of clock cycles (u16)
- `data`: 32-bit words (little-endian u32)
- `level`: SWDIO line state before clocking (0=Low, 1=High, 2=Input)
- `post`: SWDIO line state after clocking (0=Low, 1=High, 2=Input)
- `cycles`: Number of clock cycles to perform (u16)
- `speed`: SWD speed:
  - `0` = Turbo (4MHz)
  - `1` = Fast (2MHz)
  - `2` = Medium (1MHz)
  - `3` = Slow (500KHz)

## Performance Comparison

| Operation      | HTTP + JSON      | Binary Protocol |
|----------------|------------------|-----------------|
| Single Read    | ~200+ bytes      | 2 bytes req + 5 bytes resp |
| Bulk Read (4)  | ~300+ bytes      | 4 bytes req + 19 bytes resp |
| Overhead       | JSON parsing     | Zero parsing |

## Example Transactions

### Single AP Register Read (0x0C)
```
Request:  0x02 0x0C
Response: 0x00 0x12 0x34 0x56 0x78
```

### Bulk AP Register Read (4 words from 0x0C)
```
Request:  0x12 0x0C 0x04 0x00
Response: 0x00 0x04 0x00 [word0] [word1] [word2] [word3]
```

### Error Response
```
Request:  0x00 0xFF  # Invalid DP register
Response: 0x82       # Register access error
```
## Other Notes

* App AP operations apply to AP index 0
