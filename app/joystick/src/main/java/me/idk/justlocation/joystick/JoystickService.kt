package me.idk.justlocation.joystick

import android.app.*
import android.content.*
import android.graphics.*
import android.os.*
import android.util.Log
import android.view.*
import android.widget.*
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.hypot

/**
 * 悬浮摇杆：模块唤起的前台服务，自己建悬浮窗，App 内没有任何页面。
 *
 * 只有模块（面板经 KernelSU exec 调 `am`）能唤起它：`start-foreground-service` 带
 * `--ef speed <m/s>`（面板给的是 km/h，这里换算前先由面板转成 m/s，见 ui/src/joystick.ts）。
 * 关掉摇杆不会停止位置模拟；松手后位置停在原地，由后台的运动租约兜底。
 */
class JoystickService : Service() {
    private companion object { const val TAG = "JustLocationJoystick" }
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadScheduledExecutor()
    private val desired = AtomicReference(StickInput.Still)
    @Volatile private var maximumSpeed = 1.5
    @Volatile private var closed = false
    private var moving = false // worker only
    private lateinit var windows: WindowManager
    private var overlay: LinearLayout? = null
    private lateinit var label: TextView
    private lateinit var stick: StickView
    private val screenOff = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) { release() }
    }

    override fun onBind(intent: Intent?) = null

    override fun onCreate() {
        super.onCreate()
        val notifications = getSystemService(NotificationManager::class.java)
        notifications.createNotificationChannel(NotificationChannel("joystick", "悬浮摇杆", NotificationManager.IMPORTANCE_LOW))
        // 通知只作为"摇杆在运行"的可见提示，没有可打开的页面，所以不带 contentIntent。
        startForeground(1, Notification.Builder(this, "joystick")
            .setSmallIcon(android.R.drawable.ic_menu_mylocation).setContentTitle("悬浮摇杆已打开")
            .setContentText("松手后停留，关闭摇杆不会停止位置模拟")
            .setOngoing(true).build())
        windows = getSystemService(WindowManager::class.java)
        registerReceiver(screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF), RECEIVER_NOT_EXPORTED)
        worker.scheduleWithFixedDelay({
            if (!closed) {
                val input = desired.get()
                if (input.strength > 0 || moving) send(input)
            }
        }, 0, 500, TimeUnit.MILLISECONDS)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val speed = intent?.speedExtra() ?: 1.5
        if (!speed.isFinite() || speed <= 0 || speed > 1000) {
            report("invalid speed: $speed"); stopSelf(); return START_NOT_STICKY
        }
        maximumSpeed = speed
        if (overlay == null) {
            // 面板已经要求先开始模拟；这里再确认一次，避免在错误的会话状态下弹出摇杆。
            try { RootControl.requireStaticSession() } catch (error: Exception) {
                report("session not ready: ${error.javaClass.simpleName}: ${error.message}"); stopSelf(); return START_NOT_STICKY
            }
            try { showOverlay() } catch (error: Exception) {
                // 悬浮窗权限由模块安装脚本授予；走到这里说明没授上。
                report("cannot show the joystick; check the overlay permission (${error.javaClass.simpleName}: ${error.message})")
                stopSelf(); return START_NOT_STICKY
            }
        }
        if (::label.isInitialized) label.text = speedLabel()
        return START_NOT_STICKY
    }

    /**
     * 起不来时既要让用户看见，也要留下日志。
     *
     * 只弹 Toast 的话现场什么都查不到——真机验收时服务静静地自己退出了，
     * logcat 里一条记录都没有，只能靠猜。
     *
     * 日志与 Toast 都用英文（2026-09-12 用户要求）：两边都会经过终端与管理器，
     * 中文在这些链路上容易被编码搞坏。
     */
    private fun report(message: String) {
        Log.w(TAG, message)
        Toast.makeText(this, message, Toast.LENGTH_LONG).show()
    }

    private fun speedLabel() = String.format(Locale.ROOT, "最高 %.1f km/h", maximumSpeed * 3.6)

    private fun send(input: StickInput) {
        try {
            RootControl.request("drive", input.strength * maximumSpeed, input.bearing)
            moving = input.strength > 0
            main.post { if (!closed && ::label.isInitialized) label.text = speedLabel() }
        } catch (error: Exception) {
            moving = false; desired.set(StickInput.Still)
            main.post {
                if (!closed && ::stick.isInitialized) {
                    stick.reset(); label.text = error.message ?: "连接中断，已停止移动"
                }
            }
        }
    }

    private fun release() {
        desired.set(StickInput.Still)
        if (::stick.isInitialized) stick.reset()
        if (!closed) worker.execute { if (moving) send(StickInput.Still) }
    }

    private fun showOverlay() {
        fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
        val params = WindowManager.LayoutParams(dp(112), WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE, PixelFormat.TRANSLUCENT).apply {
            gravity = Gravity.TOP or Gravity.START; x = dp(12); y = dp(180)
        }
        val panel = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(6), dp(2), dp(6), dp(6))
            background = android.graphics.drawable.GradientDrawable().apply {
                setColor(Color.rgb(242, 245, 250)); cornerRadius = dp(16).toFloat()
                setStroke(dp(1), Color.rgb(199, 210, 227))
            }
            elevation = dp(8).toFloat()
        }
        val header = LinearLayout(this)
        val title = TextView(this).apply {
            text = "摇杆"; textSize = 12f; setTextColor(Color.rgb(30, 47, 68))
            gravity = Gravity.CENTER; contentDescription = "拖动摇杆，贴边收起"
        }
        header.addView(title, LinearLayout.LayoutParams(-1, dp(28)))
        val handle = TextView(this).apply {
            textSize = 22f; gravity = Gravity.CENTER; visibility = View.GONE
            contentDescription = "展开摇杆"; setTextColor(Color.rgb(70, 103, 155))
        }
        var initialX = 0; var initialY = 0; var touchX = 0f; var touchY = 0f
        title.setOnTouchListener { _, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> { initialX = params.x; initialY = params.y; touchX = event.rawX; touchY = event.rawY; release() }
                MotionEvent.ACTION_MOVE -> {
                    val bounds = windows.currentWindowMetrics.bounds
                    params.x = (initialX + event.rawX - touchX).toInt().coerceIn(0, (bounds.width() - panel.width).coerceAtLeast(0))
                    params.y = (initialY + event.rawY - touchY).toInt().coerceIn(0, (bounds.height() - panel.height - dp(32)).coerceAtLeast(0))
                    windows.updateViewLayout(panel, params)
                }
                MotionEvent.ACTION_UP -> {
                    val right = windows.currentWindowMetrics.bounds.width() - panel.width
                    if (params.x <= dp(12) || params.x >= right - dp(12)) {
                        release()
                        val onRight = params.x > right / 2
                        header.visibility = View.GONE; stick.visibility = View.GONE; label.visibility = View.GONE
                        handle.text = if (onRight) "‹" else "›"; handle.visibility = View.VISIBLE
                        params.width = dp(28); params.x = if (onRight) windows.currentWindowMetrics.bounds.width() - dp(28) else 0
                        windows.updateViewLayout(panel, params)
                        handle.setOnClickListener {
                            handle.visibility = View.GONE; header.visibility = View.VISIBLE; stick.visibility = View.VISIBLE; label.visibility = View.VISIBLE
                            params.width = dp(112); params.x = if (onRight) windows.currentWindowMetrics.bounds.width() - dp(112) else 0
                            windows.updateViewLayout(panel, params)
                        }
                    }
                }
            }
            true
        }
        panel.addView(header)
        stick = StickView(this) { input ->
            desired.set(input)
            if (input.strength == 0.0 && !closed) worker.execute { if (moving) send(StickInput.Still) }
        }
        panel.addView(stick, LinearLayout.LayoutParams(-1, dp(88)))
        label = TextView(this).apply { text = speedLabel(); textSize = 10f; gravity = Gravity.CENTER; setTextColor(Color.rgb(30, 47, 68)) }
        panel.addView(label)
        panel.addView(handle, LinearLayout.LayoutParams(-1, dp(48)))
        windows.addView(panel, params); overlay = panel
        // 成功路径也要留一条日志：只记失败时，"窗口到底加没加上"在真机上无从判断
        // （dumpsys window 里查不到这个窗口名，只能靠这条日志区分）。
        Log.i(TAG, "overlay added ${params.width}x${params.height} @ (${params.x},${params.y}) granted=${android.provider.Settings.canDrawOverlays(this)}")
    }

    override fun onDestroy() {
        closed = true; desired.set(StickInput.Still)
        unregisterReceiver(screenOff)
        overlay?.let { windows.removeView(it) }; overlay = null
        // A final release is serialized after any in-flight direction. The backend lease
        // also stops movement if Android kills this process without onDestroy.
        worker.execute { if (moving) send(StickInput.Still) }
        worker.shutdown()
        super.onDestroy()
    }
}

private class StickView(context: Context, val changed: (StickInput) -> Unit) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private var dx = 0f; private var dy = 0f
    private var pointer = -1
    private val radius get() = minOf(width, height) * 0.36f
    init { contentDescription = "方向摇杆" }
    fun reset() { pointer = -1; dx = 0f; dy = 0f; invalidate() }
    override fun onDraw(canvas: Canvas) {
        paint.color = Color.rgb(220, 230, 243)
        canvas.drawCircle(width / 2f, height / 2f, radius + 12, paint)
        paint.color = Color.rgb(169, 190, 217); paint.strokeWidth = 2f
        canvas.drawLine(width / 2f - radius, height / 2f, width / 2f + radius, height / 2f, paint)
        canvas.drawLine(width / 2f, height / 2f - radius, width / 2f, height / 2f + radius, paint)
        paint.color = Color.rgb(70, 103, 155)
        canvas.drawCircle(width / 2f + dx, height / 2f + dy, radius * 0.38f, paint)
    }
    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> pointer = event.getPointerId(0)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> { reset(); changed(StickInput.Still); return true }
            MotionEvent.ACTION_POINTER_UP -> if (event.getPointerId(event.actionIndex) == pointer) {
                reset(); changed(StickInput.Still); return true
            }
        }
        val index = event.findPointerIndex(pointer)
        if (index >= 0) {
            val x = event.getX(index) - width / 2f; val y = event.getY(index) - height / 2f
            val scale = (radius / hypot(x, y).coerceAtLeast(1f)).coerceAtMost(1f)
            dx = x * scale; dy = y * scale
            changed(StickInput.from(x, y, radius)); invalidate()
        }
        return true
    }
}
