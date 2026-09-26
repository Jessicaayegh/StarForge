#!/bin/bash
# Fix: Add binary size budget tracking for release artifacts
MAX_SIZE_MB=50
ACTUAL_SIZE_MB=$(du -m dist/release.bin | cut -f1)

if [ "$ACTUAL_SIZE_MB" -gt "$MAX_SIZE_MB" ]; then
    echo "Error: Release artifact exceeds size budget ($ACTUAL_SIZE_MB MB > $MAX_SIZE_MB MB)"
    exit 1
else
    echo "Size budget passed ($ACTUAL_SIZE_MB MB)"
fi
