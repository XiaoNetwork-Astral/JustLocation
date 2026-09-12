#!/system/bin/sh

# Installer messages remain localized for the manager UI.
MSG_NEED_KSU='请从 KernelSU 管理器安装这个模块。'
MSG_NEED_BOOTMODE='请从 KernelSU 管理器安装，不要从 recovery 安装。'
MSG_NEED_ARM64='这个构建只支持 ARM64。'
MSG_NEED_API='这个构建面向 Android 15（API 35）。'
MSG_INSTALLING='JustLocation：正在安装'
MSG_NEED_ZYGISK='需要 Zygisk Next 已启用。'
MSG_VERIFY_OK='文件完整性检查通过'
MSG_VERIFY_FAIL='文件校验不通过，安装包可能已损坏，请重新打包或重新下载。'
MSG_VERIFY_SKIPPED='没有校验清单（源码目录打包），跳过完整性检查。'
MSG_EXTRACTING='正在解压模块文件…'
MSG_EXTRACT_FAIL='解压失败，安装包可能已损坏。'
MSG_JOYSTICK_OK='摇杆 App 已安装并授予悬浮窗权限。'
MSG_JOYSTICK_FAIL='摇杆 App 安装失败，面板里的摇杆将不可用；可重装模块再试。'
MSG_JOYSTICK_OVERLAY_FAIL='摇杆 App 已安装，但悬浮窗权限没授上；请在系统设置里给它"显示在其他应用上层"，或重装模块。'
MSG_LEGACY_REMOVED='已移除旧版的合并 App。'
MSG_DONE='安装完成，重启后生效。'

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
