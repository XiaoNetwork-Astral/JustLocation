#!/system/bin/sh
# 把一条已编码的协议请求交给常驻后台并打印原始响应。
# 用法: sh justlocation-check.sh <base64(JSON)>
#
# 参数是 base64 而不是 JSON 原文：adb shell 那一层会吃掉 JSON 里的引号、花括号和逗号
# （实测 `{"version":1,"op":"status"}` 到设备上变成 `version:1 op:status`），
# 只传字母数字与 `+/=` 就不会被任何一层改写。
exec su -c "/data/adb/modules/justlocation/bin/justlocationd request $1"
