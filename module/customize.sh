# KernelSU supplies extraction, ownership and SELinux labels before this script.
# Adapted to the module layout of 5ec1cff/zygisk-module-template.
[ "$KSU" = "true" ] || abort "Install this module from KernelSU."
[ "$ARCH" = "arm64" ] || abort "This build supports ARM64 only."
[ "$API" = "35" ] || abort "This initial build targets Android 15 (API 35)."

ui_print "- JustLocation: Android 15 ARM64"
ui_print "- Requires Zygisk Next; open the panel through KernelSU WebUI."
set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/bin/justlocationd" 0 0 0755
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/uninstall.sh" 0 0 0755
