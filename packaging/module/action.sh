#!/system/bin/sh

# Report backend state without changing the session.
MODDIR=${0%/*}

STATUS_REQUEST='eyJ2ZXJzaW9uIjoxLCJvcCI6InN0YXR1cyJ9'

if ! command -v ui_print >/dev/null 2>&1; then

    ui_print() { echo "$1"; }
fi

reply=$("$MODDIR/bin/justlocationd" request "$STATUS_REQUEST" 2>/dev/null)
if [ -z "$reply" ]; then
    ui_print "! daemon not responding; check that the module is enabled and the phone has rebooted"
    exit 1
fi

flag() {
    if echo "$reply" | grep -q "\"$1\":true"; then
        echo "ready"
    else
        echo "missing"
    fi
}

running="stopped"
echo "$reply" | grep -q '"requested_active":true' && running="running"

ui_print "JustLocation status"
ui_print "simulation_session=$running"
ui_print "location_hook_ready=$(flag location_hook_ready)"
ui_print "cell_query_hook_ready=$(flag cell_query_hook_ready)"
ui_print "cell_callback_hook_ready=$(flag cell_callback_hook_ready)"
ui_print "sim_hook_ready=$(flag sim_hook_ready)"
ui_print "operator_hook_ready=$(flag operator_hook_ready)"
ui_print "gnss_hook_ready=$(flag gnss_hook_ready)"
ui_print "nmea_hook_ready=$(flag nmea_hook_ready)"
ui_print "step_hook_ready=$(flag step_hook_ready)"

ui_print "wifi_scan_hook_ready=$(flag wifi_scan_hook_ready)"
ui_print "wifi_connection_hook_ready=$(flag wifi_connection_hook_ready)"

calls=$(echo "$reply" | grep -o '"wifi_hook_calls":[0-9]*' | cut -d: -f2)
ui_print "wifi_hook_calls=${calls:-0}"

raw=$(echo "$reply" | grep -o '"gnss_raw_detail":"[^"]*"' | cut -d'"' -f4)
ui_print "gnss_raw_detail=${raw:-none}"

if echo "$reply" | grep -q '"cells_synthesized":true'; then
    ui_print "! cells_synthesized=true  (no real cell data here; these cells exist only on this device)"
fi
exit 0
