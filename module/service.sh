#!/system/bin/sh
# 模块随开机启动：起常驻后台。配置保留在 /data/adb/justlocation，卸载时不删除。
MODDIR=${0%/*}
DATA=/data/adb/justlocation

# 共用函数里有完整性校验；取不到就退回到"不校验"，不让启动因为工具缺失而失败。
. "$MODDIR/util_functions.sh" 2>/dev/null

umask 077
mkdir -p "$DATA"

# 正式包里的后台二进制带 .sha256 清单；不一致就不要启动，避免运行被改坏的程序。
if command -v verify_file >/dev/null 2>&1; then
    if ! verify_file "$MODDIR/bin/justlocationd" "$MODDIR/bin/justlocationd.sha256"; then
        echo "justlocationd 校验失败，已跳过启动" >> "$DATA/service.log"
        exit 1
    fi
fi

"$MODDIR/bin/justlocationd" serve > "$DATA/service.log" 2>&1 &
