#!/usr/bin/env python3
"""
Minimal binary API repro to find what makes STM32F411 vulnerable to reset.
Systematically test operations from the failing trace.
"""

import socket
import struct
import sys
import time

AIRFROG_IP = "192.168.0.103"
AIRFROG_PORT = 4146

def send_command(sock, cmd_data, description):
    print(f"{description}... ", end="", flush=True)
    try:
        sock.send(cmd_data)
        first_byte = sock.recv(1)
        if len(first_byte) == 0:
            print("ERROR: Connection closed")
            return False
            
        status = first_byte[0]
        if status & 0x80:
            print(f"ERROR: 0x{status:02x}")
            return False
        elif status == 0x00:
            if len(cmd_data) == 2:  # Read operation
                data = sock.recv(4)
                if len(data) == 4:
                    value = struct.unpack('<I', data)[0]
                    print(f"SUCCESS: 0x{value:08x}")
                else:
                    print("ERROR: Incomplete data")
                    return False
            else:
                print("SUCCESS")
            return True
        else:
            print(f"UNKNOWN: 0x{status:02x}")
            return False
    except Exception as e:
        print(f"ERROR: {e}")
        return False

def test_sequence(sequence_name, operations):
    print(f"\n=== Testing {sequence_name} ===")
    
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(5.0)
    
    try:
        sock.connect((AIRFROG_IP, AIRFROG_PORT))
        
        # Handshake
        version = sock.recv(1)
        if len(version) == 0 or version[0] != 0x01:
            print("Handshake failed")
            return False
        sock.send(b'\x01')
        
        # Basic initialization sequence to establish SWD communication
        if not send_command(sock, b'\xF1', "SWD Line Reset"):
            return False
        if not send_command(sock, b'\x00\x00', "DP Read IDCODE"):
            return False
        # Clear any previous error conditions in the debug port
        if not send_command(sock, b'\x01\x00' + struct.pack('<I', 0x1E), "DP Write ABORT=0x1E"):
            return False
        if not send_command(sock, b'\x00\x0C', "DP Read RDBUFF"):
            return False
        # Power up the debug domain - enables debug functionality 
        if not send_command(sock, b'\x01\x04' + struct.pack('<I', 0x50000000), "DP Write CTRL/STAT=0x50000000"):
            return False
        # Configure memory access port for 32-bit transfers
        if not send_command(sock, b'\x03\x00' + struct.pack('<I', 0x23000052), "AP Write CSW=0x23000052"):
            return False
            
        # Test operations
        for op_name, cmd_data in operations:
            if not send_command(sock, cmd_data, op_name):
                print(f"Failed during {op_name}")
                return False
        
        # System reset sequence
        # AIRCR (Application Interrupt and Reset Control Register) at 0xE000ED0C
        # Writing 0x05FA0004: bits 31:16=0x05FA (required VECTKEY), bit 2=SYSRESETREQ
        # This requests a system reset, which resets the processor core and most peripherals (but not the debug interface)
        # The problematic sequence: processor configured to halt on core reset + system reset = SWD interface lockup
        if not send_command(sock, b'\x03\x04' + struct.pack('<I', 0xE000ED0C), "AP Write TAR=0xE000ED0C (AIRCR)"):
            return False
        if not send_command(sock, b'\x03\x0C' + struct.pack('<I', 0x05FA0004), "AP Write DRW=0x05FA0004 (SYSRESETREQ)"):
            return False
            
        # Test post-reset access
        # DHCSR (Debug Halting Control and Status Register) at 0xE000EDF0
        # Try to access debug register after reset to see if SWD interface survived
        # If VC_CORERESET was set before reset, this access will fail with BadAck(7) 
        # because the processor gets stuck trying to halt after reset and SWD interface locks up
        if not send_command(sock, b'\x03\x04' + struct.pack('<I', 0xE000EDF0), "AP Write TAR=0xE000EDF0 (DHCSR)"):
            print("*** FAILED - SWD connectivity lost after reset ***")
            return False
        if not send_command(sock, b'\x02\x0C', "AP Read DRW (DHCSR content)"):
            print("*** FAILED - Cannot read DHCSR ***")
            return False
            
        print("*** PASSED - Target survived reset ***")
        return True
        
    except Exception as e:
        print(f"Connection error: {e}")
        return False
    finally:
        # Proper disconnect
        try:
            sock.send(b'\xFF')  # Disconnect command
            sock.recv(1)        # Read ack
        except:
            pass
        sock.close()

# Test 1: Just basic sequence (should pass)
test_sequence("Basic Reset", [])
time.sleep(1)

# Test 2: Add DHCSR operations from trace  
test_sequence("DHCSR Operations", [
    # DHCSR (Debug Halting Control and Status Register) at 0xE000EDF0
    # Writing 0xA05F0001: bits 31:16=0xA05F (required DBGKEY), bit 0=C_DEBUGEN
    # This enables halting debug functionality, allowing the processor to be halted and controlled by a debugger
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)),
    ("AP Write DRW=0xA05F0001 (C_DEBUGEN)", b'\x03\x0C' + struct.pack('<I', 0xA05F0001)),
    
    # DHCSR again - Writing 0xA05F0003: bits 31:16=0xA05F (DBGKEY), bit 1=C_HALT, bit 0=C_DEBUGEN  
    # This enables debug functionality AND requests the processor to halt immediately
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)), 
    ("AP Write DRW=0xA05F0003 (C_HALT|C_DEBUGEN)", b'\x03\x0C' + struct.pack('<I', 0xA05F0003)),
])
time.sleep(1)

# Test 3: Add debug register operations
test_sequence("Debug Registers", [
    # DEMCR (Debug Exception and Monitor Control Register) at 0xE000EDFC
    # Writing 0x00000001: bit 0=VC_CORERESET (Vector Catch Core Reset)
    # This configures the processor to automatically halt whenever a core reset occurs
    # This is a debug feature that allows a debugger to "catch" the processor immediately after reset
    ("AP Write TAR=0xE000EDFC (DEMCR)", b'\x03\x04' + struct.pack('<I', 0xE000EDFC)),
    ("AP Write DRW=0x00000001 (VC_CORERESET)", b'\x03\x0C' + struct.pack('<I', 0x00000001)),
    
    # DHCSR (Debug Halting Control and Status Register) at 0xE000EDF0
    # Writing 0xA05F0001: bits 31:16=0xA05F (required DBGKEY), bit 0=C_DEBUGEN
    # This enables halting debug functionality, allowing the processor to be halted and controlled by a debugger
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)),
    ("AP Write DRW=0xA05F0001 (C_DEBUGEN)", b'\x03\x0C' + struct.pack('<I', 0xA05F0001)),
])
time.sleep(1)

# Test 4: Add flash register operations from trace
test_sequence("Flash Operations", [
    # Flash control register access at 0xE0042004 (STM32F4 flash interface)
    # Writing 0x00000007 to flash control register - specific flash operation command
    # This tests whether flash programming operations make the target vulnerable to reset issues
    ("AP Write TAR=0xE0042004 (Flash)", b'\x03\x04' + struct.pack('<I', 0xE0042004)),
    ("AP Write DRW=0x00000007", b'\x03\x0C' + struct.pack('<I', 0x00000007)),
    
    # Flash memory region access at 0xE0002000
    # Reading from flash interface registers to check flash status/content
    # This verifies flash interface state before attempting reset
    ("AP Write TAR=0xE0002000 (Flash)", b'\x03\x04' + struct.pack('<I', 0xE0002000)),
    ("AP Read DRW (Flash content)", b'\x02\x0C'),
])
time.sleep(1)

# Test 5: System handler priority from trace
test_sequence("System Handler Priority", [
    # SHPR (System Handler Priority Register) at 0xE000ED30
    # Writing 0x0000001F sets priority levels for system exception handlers
    # This tests whether changing system exception priorities affects reset behavior
    ("AP Write TAR=0xE000ED30 (SHPR)", b'\x03\x04' + struct.pack('<I', 0xE000ED30)),
    ("AP Write DRW=0x0000001F", b'\x03\x0C' + struct.pack('<I', 0x0000001F)),
])
time.sleep(1)

# Test 6: Complex DHCSR sequence from failing trace
test_sequence("Complex DHCSR", [
    # This sequence replicates the complex debug register manipulation from the failing trace
    # Multiple writes to DHCSR with different debug control combinations
    
    # DHCSR write 1: Enable debug functionality (C_DEBUGEN)
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)),
    ("AP Write DRW=0xA05F0001 (C_DEBUGEN)", b'\x03\x0C' + struct.pack('<I', 0xA05F0001)),
    
    # DHCSR write 2: Enable debug + halt (C_HALT | C_DEBUGEN)  
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)),
    ("AP Write DRW=0xA05F0003 (C_HALT|C_DEBUGEN)", b'\x03\x0C' + struct.pack('<I', 0xA05F0003)),
    
    # DHCSR write 3: Additional debug control bits (0xA05F0007)
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)),
    ("AP Write DRW=0xA05F0007", b'\x03\x0C' + struct.pack('<I', 0xA05F0007)),
    
    # DHCSR write 4: More complex debug state (0xA05F000B)
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)),
    ("AP Write DRW=0xA05F000B", b'\x03\x0C' + struct.pack('<I', 0xA05F000B)),
    
    # DHCSR write 5: Final debug configuration (0xA05F000D)
    ("AP Write TAR=0xE000EDF0 (DHCSR)", b'\x03\x04' + struct.pack('<I', 0xE000EDF0)),
    ("AP Write DRW=0xA05F000D", b'\x03\x0C' + struct.pack('<I', 0xA05F000D)),
])