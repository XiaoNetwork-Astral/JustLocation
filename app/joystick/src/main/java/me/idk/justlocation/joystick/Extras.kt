package me.idk.justlocation.joystick

import android.content.Intent

/**
 * 读取面板传进来的速度（m/s）。
 *
 * 不要用 `getDoubleExtra`：`am` 的 `--ef` 放进去的是 **Float**，而
 * `Bundle.getDoubleExtra` 对 Float 不做类型转换，会**直接返回默认值**——
 * 现象是参数明明在 extras 里，读出来却是默认值，极难定位（真机上就是这么卡住的）。
 * 这里按实际收到的数值类型取，两边都能work。
 */
fun Intent.speedExtra(): Double? = when (val value = extras?.get("speed")) {
    is Float -> value.toDouble()
    is Double -> value
    is Number -> value.toDouble()
    else -> null
}
