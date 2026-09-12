#!/system/bin/sh
# KernelSU 管理器里的模块"操作"按钮：只读地报一次当前状态，不改任何东西。
#
# 模块目前**不带 Web UI**，所以这个入口就是唯一的用户可见状态面
# （刚重启完想确认后台起来了、Hook 接上了，就是看这里）。
#
# **输出一律用英文**（2026-09-12 用户要求）：这些行会经过 KernelSU 的 exec 与各种
# 终端显示，中文在这种链路上容易被编码搞坏，而状态位的名字本来就是英文。
MODDIR=${0%/*}
# {"version":1,"op":"status"} 的 base64：adb shell 与 KernelSU 的 exec 都会吃掉
# JSON 里的引号和花括号，所以请求一律走 base64（与测试脚本同一套约定）。
STATUS_REQUEST='eyJ2ZXJzaW9uIjoxLCJvcCI6InN0YXR1cyJ9'

if ! command -v ui_print >/dev/null 2>&1; then
    # 从 adb/终端直接跑时没有 ui_print，退回到 echo，方便手工调试。
    ui_print() { echo "$1"; }
fi

reply=$("$MODDIR/bin/justlocationd" request "$STATUS_REQUEST" 2>/dev/null)
if [ -z "$reply" ]; then
    ui_print "! daemon not responding; check that the module is enabled and the phone has rebooted"
    exit 1
fi

# 用 grep -o 精确取一个布尔字段。不要按逗号切字段：
# 状态里含运营商名与坐标，它们本身就可能带逗号，切出来的位置会错。
flag() {
    if echo "$reply" | grep -q "\"$1\":true"; then
        echo "ready"
    else
        echo "missing"
    fi
}

running="stopped"
echo "$reply" | grep -q '"requested_active":true' && running="running"

# 名字直接用状态回包里的字段名，和 `justlocationd request` 的原始输出、以及
# test:checks 的断言名保持一致——三处叫同一个名字，排查时不用做翻译。
ui_print "JustLocation status"
ui_print "simulation_session=$running"
ui_print "location_hook_ready=$(flag location_hook_ready)"
ui_print "cell_query_hook_ready=$(flag cell_query_hook_ready)"
ui_print "cell_callback_hook_ready=$(flag cell_callback_hook_ready)"
ui_print "sim_hook_ready=$(flag sim_hook_ready)"
ui_print "operator_hook_ready=$(flag operator_hook_ready)"
ui_print "gnss_hook_ready=$(flag gnss_hook_ready)"
ui_print "nmea_hook_ready=$(flag nmea_hook_ready)"
# Wi-Fi 两项分开报：扫描结果与连接信息是两次独立的安装，装上一项不等于另一项也在。
ui_print "wifi_scan_hook_ready=$(flag wifi_scan_hook_ready)"
ui_print "wifi_connection_hook_ready=$(flag wifi_connection_hook_ready)"
# 就绪位只说方法挂上了；回调计数才说明服务端那次调用真的走到了我们的代码。
calls=$(echo "$reply" | grep -o '"wifi_hook_calls":[0-9]*' | cut -d: -f2)
ui_print "wifi_hook_calls=${calls:-0}"
# GNSS 原始通道的逐条计数（注册数,dispatch 次数,该投次数,真投次数,送出条数,失败数,失败原因,接收方）。
raw=$(echo "$reply" | grep -o '"gnss_raw_detail":"[^"]*"' | cut -d'"' -f4)
ui_print "gnss_raw_detail=${raw:-none}"
# **当前输出的基站是不是伪造的**：那一带一条真实小区数据都没有时，装置会为虚拟位置造几个，
# 免得应用因为"周围没有基站"而退回自己的定位。造出来的编号只在本机成立、云端查不到，
# 所以这里必须明说，不能让使用者误以为读到了真实基站。
if echo "$reply" | grep -q '"cells_synthesized":true'; then
    ui_print "! cells_synthesized=true  (no real cell data here; these cells exist only on this device)"
fi
exit 0
