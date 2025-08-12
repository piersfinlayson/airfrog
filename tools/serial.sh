#!/bin/bash

python -m serial.tools.miniterm /dev/ttyUSB0 115200 --filter colorize
