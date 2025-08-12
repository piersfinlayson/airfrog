#!/bin/bash

set +e

# Build libraries
echo "-----"
echo "Building libraries"
echo "-----"
cargo build -p airfrog-core
cargo build -p airfrog-swd

# Build firmware variants
echo "-----"
echo "Building firmware variants"
echo "-----"
echo "- All features"
echo "-----"
AF_STA_SSID=ssid AF_STA_PASSWORD=password cargo build -p airfrog

echo "-----"
echo "- WiFi+WWW"
echo "-----"
AF_STA_SSID=ssid AF_STA_PASSWORD=password cargo build --no-default-features --features "wifi www" -p airfrog
echo "-----"
echo "- WiFi+REST"
echo "-----"
AF_STA_SSID=ssid AF_STA_PASSWORD=password cargo build --no-default-features --features "wifi rest" -p airfrog
#echo "-----"
#echo "- WiFi+Binary API"
#echo "-----"
#AF_STA_SSID=ssid AF_STA_PASSWORD=password cargo build --no-default-features --features "wifi bin-api" -p airfrog
echo "-----"
echo "- WiFi+REST+WWW"
echo "-----"
AF_STA_SSID=ssid AF_STA_PASSWORD=password cargo build --no-default-features --features "wifi www rest" -p airfrog

# Don't bother with these as they generate lots of warnings and the firmware
# won't do anything useful with no way to talk to it.
#cargo build --no-default-features --features "wifi httpd" -p airfrog

# Build examples
echo "-----"
echo "Building examples"
echo "-----"
AF_STA_SSID=ssid AF_STA_PASSWORD=password AIRFROG_ID=1 MQTT_BROKER_IP=1.2.3.4 cargo build --examples -p airfrog-ws

# Cargo fmt everything
echo "-----"
echo "Formatting code"
echo "-----"
cargo fmt -p airfrog-ws
cargo fmt -p airfrog-bin
cargo fmt -p airfrog-core
cargo fmt -p airfrog-swd
cargo fmt -p airfrog-util
cargo fmt -p airfrog

# Cargo clippy everything
echo "-----"
echo "Running Clippy"
echo "-----"
cargo clippy -p airfrog-bin -- -D warnings
cargo clippy -p airfrog-core -- -D warnings
cargo clippy -p airfrog-swd -- -D warnings
cargo clippy -p airfrog-util -- -D warnings
AF_STA_SSID=ssid AF_STA_PASSWORD=password cargo clippy -p airfrog -- -D warnings
AF_STA_SSID=ssid AF_STA_PASSWORD=password AIRFROG_ID=1 MQTT_BROKER_IP=1.2.3.4 cargo clippy --examples -p airfrog-ws -- -D warnings

# Cargo doc everything
echo "-----"
echo "Generating documentation"
echo "-----"
cargo doc -p airfrog-bin
cargo doc -p airfrog-core
cargo doc -p airfrog-swd
cargo doc -p airfrog-util
AF_STA_SSID=ssid AF_STA_PASSWORD=password cargo doc -p airfrog

