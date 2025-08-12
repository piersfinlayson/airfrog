#!/bin/bash

# Test script to reproduce STM32F411 SWD connectivity loss after system reset
# Replicates the exact sequence from the probe-rs trace

AIRFROG_IP="192.168.0.42"  # Change this to your airfrog IP
BASE_URL="http://${AIRFROG_IP}"

echo "=== STM32 System Reset SWD Test ==="
echo "Testing on airfrog at: $AIRFROG_IP"
echo

# Step 1: Initialize target
echo "1. Initializing target..."
curl -s -X POST "$BASE_URL/api/target/reset" | jq '.'
if [ $? -ne 0 ]; then
    echo "ERROR: Failed to initialize target"
    exit 1
fi
echo

# Step 2: Set TAR to AIRCR address (0xE000ED0C)
echo "2. Setting TAR to AIRCR address (0xE000ED0C)..."
curl -s -X POST "$BASE_URL/api/raw/ap/write/0x0/0x4" \
    -H "Content-Type: application/json" \
    -d '{"data": "0xE000ED0C"}' | jq '.'
if [ $? -ne 0 ]; then
    echo "ERROR: Failed to set TAR"
    exit 1
fi
echo

# Step 3: Write SYSRESETREQ to AIRCR via DRW (this triggers system reset)
echo "3. Writing SYSRESETREQ (0x05FA0004) to AIRCR via DRW..."
echo "   This should trigger a system reset..."
curl -s -X POST "$BASE_URL/api/raw/ap/write/0x0/0xC" \
    -H "Content-Type: application/json" \
    -d '{"data": "0x05FA0004"}' | jq '.'
if [ $? -ne 0 ]; then
    echo "ERROR: Failed to write SYSRESETREQ"
    exit 1
fi
echo

# Step 4: Set TAR to DHCSR address (0xE000EDF0)  
echo "4. Setting TAR to DHCSR address (0xE000EDF0)..."
curl -s -X POST "$BASE_URL/api/raw/ap/write/0x0/0x4" \
    -H "Content-Type: application/json" \
    -d '{"data": "0xE000EDF0"}' | jq '.'
if [ $? -ne 0 ]; then
    echo "ERROR: Failed to set TAR to DHCSR"
    echo "   This is expected - SWD connectivity lost after system reset"
    echo "   BadAck(7) indicates target not responding"
    exit 2
fi
echo

# Step 5: Try to read DHCSR via DRW (this should fail with BadAck(7))
echo "5. Attempting to read DHCSR via DRW..."
echo "   This should fail with BadAck(7)..."
curl -s -X GET "$BASE_URL/api/raw/ap/read/0x0/0xC" | jq '.'
if [ $? -ne 0 ]; then
    echo "ERROR: Failed to read DHCSR"
    echo "   This confirms the problem - SWD connectivity lost"
    echo "   Target requires power cycle to recover"
    exit 2
fi

echo "SUCCESS: The target survived the system reset"
