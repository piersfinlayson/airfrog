# airfrog Default Firmware

## Quick Start

To build and flash, run the following from the repo root:

```bash
AF_STA_SSID=your-ssid AF_STA_PASSWORD=your-password cargo run --release -p airfrog
```

## Features

- **wifi**: Enables WiFi functionality.
- **wifi-log**: Enables logging in `esp-wifi`.
- **httpd**: Enables the HTTP server.
- **rest**: Enables REST API support.
- **www**: Enables the web interface.
- **bin-api**: Enables the binary API.

Some features depend on others, for example, `rest` requires `httpd`, and `www` also requires `httpd`. `httpd` and `bin-api` require wifi.  You can enable multiple features at once.
