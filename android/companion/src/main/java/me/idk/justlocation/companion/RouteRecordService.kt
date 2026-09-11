package me.idk.justlocation.companion

import android.app.*
import android.content.Context
import android.content.Intent
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.*
import org.json.JSONObject
import java.util.concurrent.Executors

/**
 * 把真实定位录成一条路线。
 *
 * 分工：本服务只负责取真实位置并交给后台，轨迹本身由 Rust 后台累积
 * （`record_start` / `record_point` / `record_stop`），这样抽稀、上限和回放格式
 * 只有一份实现，面板与设备两侧看到的也是同一份状态。
 *
 * **模拟必须先停止**：模拟运行时系统回调里给出的是模块自己的合成位置，录下来就是
 * 一条绕回自身的轨迹。后台两个方向都会拒绝，这里在开始前也先问一次状态，把原因
 * 直接告诉用户。
 *
 * 用前台服务而不是在页面上接收：录制时用户通常要锁屏或切到地图，页面一停就收不到
 * 回调了；前台服务只挂一条低优先级通知，与摇杆服务同样的做法。
 */
class RouteRecordService : Service() {
    companion object {
        const val ACTION_START = "me.idk.justlocation.companion.RECORD_START"
        const val ACTION_STOP = "me.idk.justlocation.companion.RECORD_STOP"
        const val ACTION_STATE = "me.idk.justlocation.companion.RECORD_STATE"
        private const val CHANNEL = "route-record"
        private const val NOTIFICATION_ID = 2
    }

    private val worker = Executors.newSingleThreadExecutor()
    private lateinit var manager: LocationManager
    /** 本段录制的起点单调时钟，用来算出每个点的相对秒数。 */
    @Volatile private var origin = 0L
    @Volatile private var lastError = ""
    @Volatile private var lastMessage = ""
    @Volatile private var stopping = false

    private val listener = object : LocationListener {
        override fun onLocationChanged(location: Location) = record(location)
        override fun onProviderEnabled(provider: String) = Unit
        override fun onProviderDisabled(provider: String) = Unit
    }

    override fun onBind(intent: Intent?) = null

    override fun onCreate() {
        super.onCreate()
        manager = getSystemService(LocationManager::class.java)
        val notifications = getSystemService(NotificationManager::class.java)
        notifications.createNotificationChannel(NotificationChannel(CHANNEL, "路线录制", NotificationManager.IMPORTANCE_LOW))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> stopRecording()
            else -> beginRecording()
        }
        return START_NOT_STICKY
    }

    private fun beginRecording() {
        startForeground(NOTIFICATION_ID, Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_menu_mylocation).setContentTitle("正在录制路线")
            .setContentText("请先停止位置模拟，这里记录的是真实移动").setOngoing(true).build())
        worker.execute {
            try {
                // 先问后台：模拟还在跑就直接拒绝，免得录出一条绕回自身的轨迹。
                val state = RootControl.request("status")
                check(!state.optBoolean("requested_active")) { "请先在面板停止位置模拟，录制期间不能同时模拟" }
                RootControl.request("record_start")
                origin = SystemClock.elapsedRealtime()
                stopping = false
                lastError = ""
                lastMessage = "正在录制，走完路线后在检查页停止"
                startUpdates()
            } catch (error: Exception) {
                lastError = error.message ?: "无法开始录制"
                lastMessage = ""
                publish()
                stopSelf()
            }
        }
    }

    private fun startUpdates() {
        var started = 0
        for (provider in providers()) {
            try {
                // 每秒一次、不限最小位移：抽稀交给后台，避免在这里丢掉真实的拐弯。
                manager.requestLocationUpdates(provider, 1000L, 0f, listener, Looper.getMainLooper())
                started++
            } catch (error: Exception) {
                lastError = "$provider: ${error.message}"
            }
        }
        if (started == 0) {
            finish(Exception(lastError.ifEmpty { "没有可用的定位来源" }))
            return
        }
        publish()
    }

    private fun record(location: Location) {
        val seconds = (SystemClock.elapsedRealtime() - origin) / 1000.0
        worker.execute {
            try {
                RootControl.recordPoint(location, seconds)
                publish()
            } catch (error: Exception) {
                // 单点失败不终止录制：位置回调本来就会持续到来，丢一个点比整段作废好。
                lastError = error.message ?: "写入定位失败"
                publish()
            }
        }
    }

    private fun stopRecording() {
        if (stopping) return
        stopping = true
        worker.execute {
            manager.removeUpdates(listener)
            try {
                // record_stop 的响应里就带着成品，直接用；再调一次 record_take 只会把成品取空，
                // 于是界面报"没有录到点"而轨迹其实已经录好了。
                val state = RootControl.request("record_stop")
                lastMessage = store(state.optJSONObject("recorded"))
                lastError = ""
            } catch (error: Exception) {
                lastError = error.message ?: "结束录制失败"
                lastMessage = ""
            }
            publish()
            stopSelf()
        }
    }

    private fun finish(error: Exception) {
        lastError = error.message ?: "录制失败"
        manager.removeUpdates(listener)
        publish()
        stopSelf()
    }

    /** 把成品写到应用自己的外部目录：不需要额外权限，卸载时一并清理。 */
    private fun store(recorded: JSONObject?): String {
        if (recorded == null) return "没有录到点"
        val points = recorded.optJSONArray("points")?.length() ?: 0
        val file = java.io.File(getExternalFilesDir(null) ?: filesDir, "recorded-route.json")
        file.writeText(recorded.toString())
        return "已录 $points 个点，保存在 ${file.absolutePath}"
    }

    private fun providers(): List<String> =
        listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER).filter { manager.allProviders.contains(it) }

    private fun publish() {
        sendBroadcast(Intent(ACTION_STATE).setPackage(packageName).apply {
            putExtra("message", lastMessage)
            putExtra("error", lastError)
        })
    }

    override fun onDestroy() {
        manager.removeUpdates(listener)
        worker.shutdown()
        super.onDestroy()
    }
}
