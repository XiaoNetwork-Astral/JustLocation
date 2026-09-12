package me.idk.justlocation.joystick

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.widget.Toast

/**
 * 唤起摇杆用的空壳 Activity：透明、无界面、启动后立刻结束。
 *
 * 存在的唯一理由：系统禁止 shell 从后台启动前台服务
 * （`Background start not allowed: ... from pkg=com.android.shell startFg?=true`），
 * 但允许**处于前台的进程**启动前台服务。面板用 `am start` 打开这个 Activity，
 * 摇杆服务就由"正在前台的自己"拉起，绕开那条限制。MIUI 的「后台弹出界面」授权
 * 正好是放行这条路的开关。
 *
 * 它不做别的事：校验速度与悬浮窗权限 → 起服务 → 立刻结束自己，
 * 不进最近任务、没有界面、`noHistory`。
 */
class CallActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val speed = intent?.speedExtra() ?: -1.0
        // 日志与提示一律英文（2026-09-12 用户要求）：logcat 与 toast 都会经过终端/管理器，
        // 中文在这些链路上容易被编码搞坏。
        Log.i(TAG, "wake: speed=$speed canDrawOverlays=${Settings.canDrawOverlays(this)}")
        if (speed <= 1.0) {
            // 面板没给有效速度就什么都不做，避免用一个默认值把摇杆打开。
            Log.w(TAG, "invalid speed, ignoring this wake request")
            finishAndRemoveTask()
            return
        }
        if (!Settings.canDrawOverlays(this)) {
            // 正常安装时会由脚本授予；走到这里说明没授上，给用户一个能自己去开的入口。
            Log.w(TAG, "missing overlay permission")
            Toast.makeText(this, "Allow JustLoystick to draw over other apps first", Toast.LENGTH_LONG).show()
            startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, android.net.Uri.parse("package:$packageName")))
            finishAndRemoveTask()
            return
        }
        try {
            startForegroundService(Intent(this, JoystickService::class.java).putExtra("speed", speed))
            Log.i(TAG, "joystick service start requested")
        } catch (error: Exception) {
            // 前台服务被系统拒绝时不会抛异常，只会静默失败；这里至少把已发生的事记下来。
            Log.w(TAG, "service start failed: ${error.javaClass.simpleName}: ${error.message}")
        }
        finishAndRemoveTask()
    }

    private companion object { const val TAG = "JustLocationJoystick" }
}
