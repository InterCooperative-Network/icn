#!/usr/bin/env bash
# Usage: move_module.sh <path/in/legacy> <path/in/new>
legacy_path="legacy/icn-legacy/$1"
dest="crates/$2"
mkdir -p "$(dirname $dest)"
cp -R "$legacy_path" "$dest" 