#!/system/bin/sh
# Forward one Base64-encoded JSON request to the daemon.

exec su -c "/data/adb/modules/justlocation/bin/justlocationd request $1"
