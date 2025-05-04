#!/bin/bash
sed -i "s/Trap::new/Trap::throw/g" crates/core-vm/src/host_abi.rs
echo "Updated all instances of Trap::new to Trap::throw"
