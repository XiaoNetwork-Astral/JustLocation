#!/system/bin/sh
# 模块脚本共用的小工具。
#
# 这里只放三件事：面向用户的文字（集中在一处，将来接多语言只需换这一个文件）、
# 安装期会反复用到的判断，以及模块自带的 App 的包名。安装脚本与运行脚本都从这里取，
# 避免同一段判断在几个脚本里各写一遍、改一处漏一处。

# ---- 面向用户的文字 ----
# 变量名保持 MSG_ 前缀，取值集中在这里；当前只有中文。
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

# 模块自带的摇杆 App（显示名 JustLoystick）。没有启动器与页面，全部入口由面板唤起。
# 它要调 su 把摇杆动作交给后台，所以**必须由用户在 KernelSU 管理器里授权一次 root**——
# allowlist 是内核管理的，没有任何命令行能添加（安装脚本只能装上并授悬浮窗权限）。
JOYSTICK_PACKAGE='me.idk.justlocation.joystick'
# 旧版把摇杆与自检放在同一个包里；升级时清掉它。
LEGACY_PACKAGE='me.idk.justlocation.companion'

msg() {
    ui_print "- $1"
}

# KernelSU 自带 grep_prop，但源码目录安装等场景下可能取不到；这里补一个等价实现。
if ! command -v grep_prop >/dev/null 2>&1; then
    grep_prop() {
        sed -n "s/^$1=//p" "$2" 2>/dev/null | head -n 1
    }
fi

# abort 由 KernelSU 提供；这里包一层，保证失败信息也带上前缀。
fail() {
    abort "! $1"
}

require_arm64() {
    [ "$ARCH" = "arm64" ] || fail "$MSG_NEED_ARM64"
}

require_kernelsu() {
    [ "$KSU" = "true" ] || fail "$MSG_NEED_KSU"
}

# 从 recovery 安装时 BOOTMODE 为 false：这时模块目录不是真正的模块位置，
# 装出来的东西状态不对，宁可直接拒绝（LSPosed 也是这么做的）。
require_bootmode() {
    [ "$BOOTMODE" = "true" ] || fail "$MSG_NEED_BOOTMODE"
}

require_api() {
    [ "$API" = "35" ] || fail "$MSG_NEED_API"
}

# ---- 运行期校验 ----
# 安装期的整包校验在 verify.sh（读 ZIP 里的字节）；这里是给 service.sh 用的：
# 启动后台之前确认模块目录里的二进制没被动过，坏文件就不要运行。

# sha256_of <文件>：打印十六进制摘要，取不到时返回空。
sha256_of() {
    [ -f "$1" ] || return 1
    sha256sum "$1" 2>/dev/null | cut -d' ' -f1
}

# verify_file <文件> <期望摘要文件>
# 期望摘要缺失时视为通过：这样源码目录里的模块仍可运行，而带清单的正式包会被真正校验。
verify_file() {
    [ -f "$1" ] || return 1
    [ -f "$2" ] || return 0
    _actual=$(sha256_of "$1") || return 1
    _expected=$(cut -d' ' -f1 < "$2" 2>/dev/null)
    [ -n "$_actual" ] && [ "$_actual" = "$_expected" ]
}
