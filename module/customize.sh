# 安装脚本。
#
# 打包与安装的骨架参考了 LSPosed 那套模块的做法：ZIP 平铺、每个文件带校验、
# SKIPUNZIP=1 由脚本自己解压。这样做的**实际好处**是校验发生在落盘之前——
# KernelSU 默认会把整个 ZIP 先解到模块目录再让脚本校验，包坏了也已经写进去了。
#
# 与 LSPosed 的差异（按我们自己的逻辑取舍）：
#   - 只支持 arm64，不做多 ABI 分支；
#   - 没有签名工具与在线许可，因此没有那一整套 abort 分支；
#   - 摇杆 App 随模块安装、随模块卸载，用 pm 而不是自研 daemon；
#   - 校验清单是单文件 `checksums`，不是每个文件各配一个 `.sha256`。
SKIPUNZIP=1

# 先把要用的脚本从 ZIP 里解出来（此时模块目录里什么都没有，因为跳过了自动解压）。
# 这一步失败说明 ZIP 本身不可读或缺少必需文件，没必要继续。
unzip -o "$ZIPFILE" 'verify.sh' 'util_functions.sh' 'checksums' -d "$MODPATH" >/dev/null 2>&1 || true

. "$MODPATH/util_functions.sh"
. "$MODPATH/verify.sh"

require_kernelsu
require_bootmode
require_arm64
require_api

# module.prop 此时**还没解出来**（跳过了自动解压），所以从 ZIP 里读版本号。
msg "$MSG_INSTALLING ($(unzip -p "$ZIPFILE" module.prop 2>/dev/null | sed -n 's/^version=//p'))"

if [ -f "$MODPATH/checksums" ]; then
    if verified=$(verify_zip "$ZIPFILE" "$MODPATH/checksums"); then
        msg "$MSG_VERIFY_OK（$verified）"
    else
        # verify_zip 会把第一个失败的文件名打出来，避免只说一句"校验失败"。
        msg "$verified"
        fail "$MSG_VERIFY_FAIL"
    fi
else
    # 直接从源码目录打包、没有生成清单时走到这里；与项目里"清单缺失按通过处理"的约定一致。
    msg "$MSG_VERIFY_SKIPPED"
fi

msg "$MSG_EXTRACTING"
unzip -o "$ZIPFILE" -x 'META-INF/*' -d "$MODPATH" >/dev/null 2>&1 || fail "$MSG_EXTRACT_FAIL"

# 模块自带的摇杆 App：没有启动器、也没有可打开的页面，只能由面板唤起。
# 先卸掉上一版，避免签名或版本号不同导致 -r 安装失败。
if [ -f "$MODPATH/bin/joystick.apk" ]; then
    pm uninstall "$JOYSTICK_PACKAGE" >/dev/null 2>&1
    if pm install -g -r "$MODPATH/bin/joystick.apk" >/dev/null 2>&1; then
        # 悬浮窗权限是 appop 而不是运行时权限，pm install -g 不会授予，必须单独放行。
        # 刚装完的包有时还没在 appops 里登记，所以要重试几次并**核对结果**：
        # 早先的写法把输出全丢了，装完看着像成功、实际权限没授上（真机验收就是这么发现的）。
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

# 上一版把摇杆和自检放在同一个 companion 包里；升级时清掉，
# 免得系统里留一个带启动器图标的旧应用。
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
