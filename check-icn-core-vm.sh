#!/bin/bash
cd /home/matt/dev/icn/icn-runtime-root
cargo check --package icn-core-vm --verbose
echo "Exit code: $?" 