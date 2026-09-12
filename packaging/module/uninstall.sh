#!/system/bin/sh
MODDIR=${0%/*}
# Configuration is retained so reinstalling does not discard saved coordinates.
"$MODDIR/bin/justlocationd" request eyJ2ZXJzaW9uIjoxLCJvcCI6InNodXRkb3duIn0= >/dev/null 2>&1

# 模块自带的摇杆 App 跟着模块一起消失。
# KernelSU 的 prune_modules() 会在**删除模块目录之前**执行这个脚本（userspace/ksud/src/module.rs），
# 所以这里还能读到 $MODDIR；再晚一步 App 就会留在系统里。
. "$MODDIR/util_functions.sh" 2>/dev/null
pm uninstall "${JOYSTICK_PACKAGE:-me.idk.justlocation.joystick}" >/dev/null 2>&1

exit 0
