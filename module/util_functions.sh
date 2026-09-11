#!/system/bin/sh
# 模块脚本共用的小工具。
#
# 这里只放两件事：面向用户的文字（集中在一处，将来接多语言只需换这一个文件），
# 以及安装期会反复用到的判断和完整性校验。安装脚本与运行脚本都从这里取函数，
# 避免同一段判断在几个脚本里各写一遍、改一处漏一处。

# ---- 面向用户的文字 ----
# 变量名保持 MSG_ 前缀，取值集中在这里；当前只有中文。
MSG_NEED_KSU='请从 KernelSU 管理器安装这个模块。'
MSG_NEED_ARM64='这个构建只支持 ARM64。'
MSG_NEED_API='这个构建面向 Android 15（API 35）。'
MSG_INSTALLING='JustLocation：正在安装'
MSG_NEED_ZYGISK='需要 Zygisk Next 已启用；面板从 KernelSU 的 WebUI 打开。'
MSG_VERIFY_OK='文件完整性检查通过。'
MSG_VERIFY_FAIL='文件校验不通过，安装包可能已损坏，请重新打包或重新下载。'
MSG_DONE='安装完成，重启后生效。'

msg() {
    ui_print "- $1"
}

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

require_api() {
    [ "$API" = "35" ] || fail "$MSG_NEED_API"
}

# sha256_of <文件>：打印十六进制摘要，取不到时返回空。
sha256_of() {
    [ -f "$1" ] || return 1
    sha256sum "$1" 2>/dev/null | cut -d' ' -f1
}

# verify_file <文件> <期望摘要文件>
# 期望摘要缺失时视为通过：这样源码目录里的模块仍可安装，而带清单的正式包会被真正校验。
verify_file() {
    [ -f "$1" ] || return 1
    [ -f "$2" ] || return 0
    actual=$(sha256_of "$1") || return 1
    expected=$(cat "$2" 2>/dev/null | cut -d' ' -f1)
    [ -n "$actual" ] && [ "$actual" = "$expected" ]
}

# verify_tree <目录>：逐个核对目录下的 .sha256 清单。
# 打印出第一个校验失败的文件，便于定位。
verify_tree() {
    _dir=$1
    for _manifest in $(find "$_dir" -name '*.sha256' 2>/dev/null); do
        _target=${_manifest%.sha256}
        if ! verify_file "$_target" "$_manifest"; then
            msg "校验失败：$_target"
            return 1
        fi
    done
    return 0
}
