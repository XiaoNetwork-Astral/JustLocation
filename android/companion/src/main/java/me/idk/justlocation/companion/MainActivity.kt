package me.idk.justlocation.companion

import android.app.Activity
import android.Manifest
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Bundle
import android.os.CancellationSignal
import android.os.Looper
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.util.Locale

class MainActivity : Activity() {
    private lateinit var manager: LocationManager
    private lateinit var output: TextView
    private lateinit var status: TextView
    private lateinit var recordingStatus: TextView
    private val results = ArrayDeque<String>()
    private val pending = mutableListOf<CancellationSignal>()
    private var permissionAction: (() -> Unit)? = null
    /** 录制服务把进度与结果广播回来，页面只负责显示。 */
    private val recorder = object : android.content.BroadcastReceiver() {
        override fun onReceive(context: android.content.Context?, intent: android.content.Intent?) {
            val message = intent?.getStringExtra("message").orEmpty()
            val error = intent?.getStringExtra("error").orEmpty()
            recordingStatus.text = when {
                error.isNotEmpty() -> "路线录制：$error"
                message.isNotEmpty() -> "路线录制：$message"
                else -> "路线录制：进行中"
            }
        }
    }
    private val listener = object : LocationListener {
        override fun onLocationChanged(location: Location) = showLocation("持续回调", location)
        override fun onProviderEnabled(provider: String) = log("$provider 已启用")
        override fun onProviderDisabled(provider: String) = log("$provider 已关闭")
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        manager = getSystemService(LocationManager::class.java)
        val padding = (24 * resources.displayMetrics.density).toInt()
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(padding, padding * 2, padding, padding)
        }
        fun text(value: String, size: Float = 15f) = TextView(this).apply {
            text = value; textSize = size; setPadding(0, 12, 0, 16); content.addView(this)
        }
        text("JustLocation", 28f)
        text("定位接收检查", 21f)
        text("将本应用加入面板的作用范围，再检查开始、换点、停止后的结果。这里只读取 Android 定位接口，不从模拟后台读取坐标。")
        text(packageName, 12f)
        status = text("尚未请求定位")
        fun button(label: String, action: () -> Unit) {
            content.addView(Button(this).apply { text = label; setOnClickListener { action() } })
        }
        button("读取最近位置") { permitted { providers().forEach { provider ->
            try { showLocation("最近位置 · $provider", manager.getLastKnownLocation(provider)) }
            catch (error: Exception) { log("$provider: ${error.message}") }
        } } }
        button("请求单次定位") { permitted {
            pending.forEach { it.cancel() }; pending.clear()
            providers().forEach { provider ->
                val signal = CancellationSignal(); pending.add(signal)
                try {
                    manager.getCurrentLocation(provider, signal, mainExecutor) { location ->
                        if (pending.remove(signal)) {
                            showLocation("单次定位 · $provider", location)
                            updateSingleStatus()
                        }
                    }
                } catch (error: Exception) { pending.remove(signal); log("$provider: ${error.message}") }
            }
            updateSingleStatus()
        } }
        button("开始接收持续定位") { permitted {
            manager.removeUpdates(listener)
            providers().forEach { provider ->
                try { manager.requestLocationUpdates(provider, 1000, 0f, listener, Looper.getMainLooper()) }
                catch (error: Exception) { log("$provider: ${error.message}") }
            }
            status.text = "正在接收定位；离开此页面会停止接收"
        } }
        button("停止接收") { stopReceiving() }
        button("清空结果") { results.clear(); output.text = "暂无结果" }
        button("悬浮摇杆") { startActivity(android.content.Intent(this, JoystickActivity::class.java)) }
        // 路线录制：取真实移动交给后台，成品落到本应用的外部目录。
        recordingStatus = text("路线录制：未开始")
        button("开始录制路线") { permitted {
            // 录制期间系统回调必须是真实位置，所以先确认模拟已停止。
            if (!simulationStopped()) return@permitted
            startForegroundService(android.content.Intent(this, RouteRecordService::class.java))
            recordingStatus.text = "路线录制：正在录制，走完后再点停止"
        } }
        button("停止录制并保存") {
            startService(android.content.Intent(this, RouteRecordService::class.java).setAction(RouteRecordService.ACTION_STOP))
            recordingStatus.text = "路线录制：正在结束…"
        }
        output = text("暂无结果", 13f).apply { setTextIsSelectable(true) }
        setContentView(ScrollView(this).apply { addView(content) })
    }

    private fun providers(): List<String> = listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)
        .filter { manager.allProviders.contains(it) }

    /**
     * 录制前确认模拟已停止。
     *
     * 模拟运行时系统回调给出的是模块的合成位置，录下来就是一条绕回自身的轨迹；后台也会拒绝，
     * 但在这里先问一次能把原因当场说清楚，而不是等录制服务报错。
     */
    private fun simulationStopped(): Boolean {
        return try {
            val state = RootControl.request("status")
            if (state.optBoolean("requested_active")) {
                recordingStatus.text = "路线录制：请先在面板停止位置模拟"
                false
            } else true
        } catch (error: Exception) {
            recordingStatus.text = "路线录制：${error.message ?: "无法读取模块状态"}"
            false
        }
    }

    override fun onStart() {
        super.onStart()
        registerReceiver(recorder, android.content.IntentFilter(RouteRecordService.ACTION_STATE), RECEIVER_NOT_EXPORTED)
    }

    override fun onStop() {
        try { unregisterReceiver(recorder) } catch (_: IllegalArgumentException) { }
        stopReceiving()
        super.onStop()
    }

    private fun updateSingleStatus() {
        status.text = if (pending.isEmpty()) "单次定位已结束" else "正在等待单次定位结果"
    }

    private fun permitted(action: () -> Unit) {
        if (checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED) action()
        else {
            permissionAction = action
            requestPermissions(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION), 1)
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        val action = permissionAction; permissionAction = null
        if (requestCode == 1 && checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED) {
            action?.invoke()
        } else status.text = "需要位置权限才能检查实际输出"
    }

    private fun showLocation(source: String, location: Location?) {
        if (location == null) { log("$source：没有可用位置"); return }
        log(String.format(Locale.ROOT, "%s\n%s  %.6f, %.6f\n海拔 %.1f m · 精度 %.1f m · mock=%s\ntime=%d · elapsed=%d",
            source, location.provider, location.latitude, location.longitude, location.altitude,
            location.accuracy, location.isMock, location.time, location.elapsedRealtimeNanos))
    }

    private fun log(value: String) {
        results.addFirst(value)
        while (results.size > 40) results.removeLast()
        output.text = results.joinToString("\n\n")
    }

    private fun stopReceiving() {
        manager.removeUpdates(listener)
        pending.forEach { it.cancel() }; pending.clear()
        status.text = "已停止接收"
    }
}
