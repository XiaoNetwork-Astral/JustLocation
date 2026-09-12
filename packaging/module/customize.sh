# Extract the verification tools first, then verify the payload before installing it.
SKIPUNZIP=1

unzip -o "$ZIPFILE" 'verify.sh' 'util_functions.sh' 'checksums' -d "$MODPATH" >/dev/null 2>&1 || true

. "$MODPATH/util_functions.sh"
. "$MODPATH/verify.sh"

require_kernelsu
require_bootmode
require_arm64
require_api

# module.prop is still inside the ZIP at this point.
msg "$MSG_INSTALLING ($(unzip -p "$ZIPFILE" module.prop 2>/dev/null | sed -n 's/^version=//p'))"

if [ -f "$MODPATH/checksums" ]; then
    if verified=$(verify_zip "$ZIPFILE" "$MODPATH/checksums"); then
        msg "$MSG_VERIFY_OK（$verified）"
    else

        msg "$verified"
        fail "$MSG_VERIFY_FAIL"
    fi
else

    msg "$MSG_VERIFY_SKIPPED"
fi

msg "$MSG_EXTRACTING"
unzip -o "$ZIPFILE" -x 'META-INF/*' -d "$MODPATH" >/dev/null 2>&1 || fail "$MSG_EXTRACT_FAIL"

# Replace the companion APK to handle signing or version changes.
if [ -f "$MODPATH/bin/joystick.apk" ]; then
    pm uninstall "$JOYSTICK_PACKAGE" >/dev/null 2>&1
    if pm install -g -r "$MODPATH/bin/joystick.apk" >/dev/null 2>&1; then

        # Overlay permission is an app-op; verify it after package registration settles.
        overlay_granted=false
        for _attempt in 1 2 3 4 5; do
            appops set "$JOYSTICK_PACKAGE" SYSTEM_ALERT_WINDOW allow >/dev/null 2>&1
            case "$(appops get "$JOYSTICK_PACKAGE" SYSTEM_ALERT_WINDOW 2>/dev/null)" in
                *allow*) overlay_granted=true; break ;;
            esac
            sleep 1
        done
        if [ "$overlay_granted" = "true" ]; then
            msg "$MSG_JOYSTICK_OK"
        else
            msg "$MSG_JOYSTICK_OVERLAY_FAIL"
        fi
    else
        msg "$MSG_JOYSTICK_FAIL"
    fi
fi

# Remove the obsolete combined companion package on upgrade.
if pm path "$LEGACY_PACKAGE" >/dev/null 2>&1; then
    pm uninstall "$LEGACY_PACKAGE" >/dev/null 2>&1
    msg "$MSG_LEGACY_REMOVED"
fi

set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/bin/justlocationd" 0 0 0755
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/action.sh" 0 0 0755
set_perm "$MODPATH/uninstall.sh" 0 0 0755
set_perm "$MODPATH/util_functions.sh" 0 0 0644

msg "$MSG_DONE"
