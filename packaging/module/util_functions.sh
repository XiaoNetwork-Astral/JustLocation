#!/system/bin/sh

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
    [ "$ARCH" = "arm64" ] || fail "Unsupported architecture: $ARCH (requires arm64)"
}

require_kernelsu() {
    [ "$KSU" = "true" ] || fail "KernelSU is required; install through KernelSU Manager"
}

require_bootmode() {
    [ "$BOOTMODE" = "true" ] || fail "Recovery installation is not supported; use KernelSU Manager"
}

require_api() {
    [ "$API" = "35" ] || fail "Unsupported Android API: $API (requires 35)"
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
