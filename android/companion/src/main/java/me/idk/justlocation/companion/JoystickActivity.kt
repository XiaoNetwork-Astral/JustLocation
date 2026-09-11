package me.idk.justlocation.companion

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.widget.*
import java.util.concurrent.Executors

/** The panel owns settings and start/stop. This page only completes first-use permissions. */
class JoystickActivity : Activity() {
    private val worker = Executors.newSingleThreadExecutor()
    private lateinit var status: TextView
    private lateinit var retry: Button
    private var waitingForOverlay = false
    private var connecting = false
    private var requestedSpeed = 0.0
    private var pendingOpen = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val padding = (24 * resources.displayMetrics.density).toInt()
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(padding, padding * 2, padding, padding)
        }
        fun text(value: String, size: Float = 16f) = TextView(this).apply {
            text = value; textSize = size; setPadding(0, 12, 0, 20); content.addView(this)
        }
        text("悬浮摇杆", 28f)
        text("在模块面板设置速度、打开或关闭摇杆。拖到屏幕边缘可收起，点把手展开。")
        status = text("正在准备摇杆…")
        retry = Button(this).apply {
            text = "继续"; setOnClickListener { connect() }; content.addView(this)
        }
        pendingOpen = intent.getBooleanExtra("open", false)
        requestedSpeed = intent.getStringExtra("speed")?.toDoubleOrNull()?.div(3.6) ?: 0.0
        if (!pendingOpen || !requestedSpeed.isFinite() || requestedSpeed <= 0 || requestedSpeed > 1000) {
            pendingOpen = false; status.text = "请从模块面板打开摇杆。"; retry.visibility = android.view.View.GONE
        }
        setContentView(content)
    }

    override fun onResume() {
        super.onResume()
        if (!pendingOpen || connecting) return
        if (waitingForOverlay) {
            waitingForOverlay = false
            if (!Settings.canDrawOverlays(this)) {
                status.text = "需要允许悬浮窗，授权后会自动打开摇杆。"; return
            }
        }
        connect()
    }

    private fun connect() {
        if (!pendingOpen || connecting) return
        if (!Settings.canDrawOverlays(this)) {
            waitingForOverlay = true
            status.text = "请允许 JustLocation 显示悬浮窗，返回后继续。"
            startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:$packageName")))
            return
        }
        connecting = true; retry.isEnabled = false; status.text = "正在连接模块，请允许 Root 授权…"
        worker.execute {
            val error = runCatching { RootControl.requireStaticSession() }.exceptionOrNull()
            runOnUiThread {
                if (isDestroyed) return@runOnUiThread
                connecting = false; retry.isEnabled = true
                if (error != null) status.text = error.message
                else {
                    pendingOpen = false
                    startForegroundService(Intent(this, JoystickService::class.java).putExtra("speed", requestedSpeed))
                    finish()
                }
            }
        }
    }
    override fun onDestroy() { worker.shutdown(); super.onDestroy() }
}
