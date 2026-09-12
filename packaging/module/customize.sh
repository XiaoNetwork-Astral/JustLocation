# KernelSU supplies ui_print, abort, device information and permission helpers.
SKIPUNZIP=1

ui_print "- Extracting installer tools"
for tool in verify.sh util_functions.sh checksums; do
    unzip -o "$ZIPFILE" "$tool" -d "$MODPATH" >/dev/null 2>&1 || abort "! Cannot extract $tool from ZIP"
done
[ -f "$MODPATH/util_functions.sh" ] || abort "! Missing util_functions.sh in ZIP"
[ -f "$MODPATH/verify.sh" ] || abort "! Missing verify.sh in ZIP"
. "$MODPATH/util_functions.sh"
. "$MODPATH/verify.sh"

version=$(unzip -p "$ZIPFILE" module.prop 2>/dev/null | sed -n 's/^version=//p')
msg "JustLocation version $version"
msg "Android API: $API"
msg "Device platform: $ARCH"
require_kernelsu
require_bootmode
require_arm64
require_api
if [ -n "$KSU_VER" ]; then msg "KernelSU version: $KSU_VER ($KSU_VER_CODE)"; fi

msg "Verifying module files"
if verified=$(verify_zip "$ZIPFILE" "$MODPATH/checksums"); then
    msg "$verified"
else
    fail "$verified"
fi

msg "Extracting backend, Zygisk library and bridge"
unzip -o "$ZIPFILE" -x 'META-INF/*' -d "$MODPATH" >/dev/null 2>&1 || fail "Cannot extract module files from ZIP"

# Preserve Package Manager's reason instead of replacing it with a generic retry message.
package_error() {
    while IFS= read -r line; do
        [ -z "$line" ] || ui_print "! $line"
    done <<EOF
$1
EOF
}

msg "Installing JustLoystick ($JOYSTICK_PACKAGE)"
installed=false
if pm_output=$(pm install -g -r "$MODPATH/bin/joystick.apk" 2>&1); then
    installed=true
else
    package_error "$pm_output"
    # Only a signing-key mismatch needs removal. Other failures must keep the existing app.
    case "$pm_output" in
        *INSTALL_FAILED_UPDATE_INCOMPATIBLE*)
            msg "Reinstalling JustLoystick (signature mismatch)"
            if pm_output=$(pm uninstall "$JOYSTICK_PACKAGE" 2>&1); then
                if pm_output=$(pm install -g -r "$MODPATH/bin/joystick.apk" 2>&1); then
                    installed=true
                else
                    package_error "$pm_output"
                fi
            else
                package_error "$pm_output"
            fi
            ;;
    esac
fi

if [ "$installed" = "true" ]; then
    msg "Granting overlay permission"
    overlay_granted=false
    for _attempt in 1 2 3 4 5; do
        overlay_error=$(appops set "$JOYSTICK_PACKAGE" SYSTEM_ALERT_WINDOW allow 2>&1)
        overlay_state=$(appops get "$JOYSTICK_PACKAGE" SYSTEM_ALERT_WINDOW 2>&1)
        case "$overlay_state" in
            *"SYSTEM_ALERT_WINDOW: allow"*) overlay_granted=true; break ;;
        esac
        sleep 1
    done
    if [ "$overlay_granted" = "true" ]; then
        msg "Overlay permission: allowed"
    else
        package_error "$overlay_error"
        package_error "$overlay_state"
        ui_print "! Enable 'Display over other apps' for JustLoystick in Android settings"
    fi
    msg "Grant JustLoystick Root access in KernelSU > Superuser"
else
    ui_print "! JustLoystick was not installed; recording and overlay controls are unavailable"
fi

if pm path "$LEGACY_PACKAGE" >/dev/null 2>&1; then
    msg "Removing $LEGACY_PACKAGE"
    if ! pm_output=$(pm uninstall "$LEGACY_PACKAGE" 2>&1); then package_error "$pm_output"; fi
fi

msg "Setting file permissions"
set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/bin/justlocationd" 0 0 0755
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/action.sh" 0 0 0755
set_perm "$MODPATH/uninstall.sh" 0 0 0755
set_perm "$MODPATH/util_functions.sh" 0 0 0644

msg "Reboot to load the module"
