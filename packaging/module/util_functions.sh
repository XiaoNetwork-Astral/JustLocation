#!/system/bin/sh

# Installation output also serves as a diagnostic log and stays in English.
MSG_NEED_KSU='Install this module from the KernelSU manager.'
MSG_NEED_BOOTMODE='Install from the KernelSU manager, not from recovery.'
MSG_NEED_ARM64='This build supports ARM64 only.'
MSG_NEED_API='This build targets Android 15 (API 35).'
MSG_INSTALLING='JustLocation: installing'
MSG_NEED_ZYGISK='Zygisk Next must be enabled.'
MSG_VERIFY_OK='File integrity check passed'
MSG_VERIFY_FAIL='File verification failed. Rebuild or download the package again.'
MSG_VERIFY_SKIPPED='No checksum manifest found; skipping integrity checks for this source archive.'
MSG_EXTRACTING='Extracting module files...'
MSG_EXTRACT_FAIL='Extraction failed; the installation package may be damaged.'
MSG_JOYSTICK_OK='Joystick app installed and overlay permission granted.'
MSG_JOYSTICK_FAIL='Joystick app installation failed. Reinstall the module to retry.'
MSG_JOYSTICK_OVERLAY_FAIL='Joystick app installed, but overlay permission is missing. Allow it to display over other apps in system settings, or reinstall the module.'
MSG_LEGACY_REMOVED='Removed the obsolete combined app.'
MSG_DONE='Installation complete. Reboot to apply changes.'

# Root access must be granted by the user in KernelSU.
JOYSTICK_PACKAGE='me.idk.justlocation.joystick'

LEGACY_PACKAGE='me.idk.justlocation.companion'

msg() {
    ui_print "- $1"
}

if ! command -v grep_prop >/dev/null 2>&1; then
    grep_prop() {
        sed -n "s/^$1=//p" "$2" 2>/dev/null | head -n 1
    }
fi

fail() {
    abort "! $1"
}

require_arm64() {
    [ "$ARCH" = "arm64" ] || fail "$MSG_NEED_ARM64"
}

require_kernelsu() {
    [ "$KSU" = "true" ] || fail "$MSG_NEED_KSU"
}

require_bootmode() {
    [ "$BOOTMODE" = "true" ] || fail "$MSG_NEED_BOOTMODE"
}

require_api() {
    [ "$API" = "35" ] || fail "$MSG_NEED_API"
}

sha256_of() {
    [ -f "$1" ] || return 1
    sha256sum "$1" 2>/dev/null | cut -d' ' -f1
}

verify_file() {
    [ -f "$1" ] || return 1
    [ -f "$2" ] || return 0
    _actual=$(sha256_of "$1") || return 1
    _expected=$(cut -d' ' -f1 < "$2" 2>/dev/null)
    [ -n "$_actual" ] && [ "$_actual" = "$_expected" ]
}
