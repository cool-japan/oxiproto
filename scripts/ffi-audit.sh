#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if cargo tree 2>/dev/null | grep -E '(-sys v|protoc-bin-vendored v|protoc-prebuilt v)'; then
    echo "FFI LEAK DETECTED in oxiproto"
    exit 1
fi
echo "oxiproto FFI audit: CLEAN"
