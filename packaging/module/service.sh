#!/system/bin/sh

# Start the daemon at boot; configuration is retained across reinstalls.
MODDIR=${0%/*}
DATA=/data/adb/justlocation

. "$MODDIR/util_functions.sh" 2>/dev/null

umask 077
mkdir -p "$DATA"

if command -v verify_file >/dev/null 2>&1; then
    if ! verify_file "$MODDIR/bin/justlocationd" "$MODDIR/bin/justlocationd.sha256"; then
        echo "justlocationd checksum mismatch; startup skipped" >> "$DATA/service.log"
        exit 1
    fi
fi

"$MODDIR/bin/justlocationd" serve > "$DATA/service.log" 2>&1 &
