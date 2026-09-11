#!/system/bin/sh
MODDIR=${0%/*}
DATA=/data/adb/justlocation
umask 077
mkdir -p "$DATA"
"$MODDIR/bin/justlocationd" serve > "$DATA/service.log" 2>&1 &
