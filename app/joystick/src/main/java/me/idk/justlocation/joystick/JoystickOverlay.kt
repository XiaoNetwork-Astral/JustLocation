package me.idk.justlocation.joystick

import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.LinearLayout
import android.widget.TextView

/** Owns the floating window and its touch interactions on the main thread. */
internal class JoystickOverlay(
    private val context: Context,
    private val changed: (StickInput) -> Unit,
    private val release: () -> Unit,
) {
    private val windows = context.getSystemService(WindowManager::class.java)
    private var panel: LinearLayout? = null
    private lateinit var label: TextView
    private lateinit var stick: StickView

    fun show(labelText: String) {
        fun dp(value: Int) = (value * context.resources.displayMetrics.density).toInt()
        val params =
            WindowManager.LayoutParams(
                    dp(112),
                    WindowManager.LayoutParams.WRAP_CONTENT,
                    WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                    WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE,
                    PixelFormat.TRANSLUCENT,
                )
                .apply {
                    gravity = Gravity.TOP or Gravity.START
                    x = dp(12)
                    y = dp(180)
                }
        val panel =
            LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(dp(6), dp(2), dp(6), dp(6))
                background =
                    GradientDrawable().apply {
                        setColor(Color.rgb(242, 245, 250))
                        cornerRadius = dp(16).toFloat()
                        setStroke(dp(1), Color.rgb(199, 210, 227))
                    }
                elevation = dp(8).toFloat()
            }
        val header = LinearLayout(context)
        val title =
            TextView(context).apply {
                text = "摇杆"
                textSize = 12f
                setTextColor(Color.rgb(30, 47, 68))
                gravity = Gravity.CENTER
                contentDescription = "拖动摇杆，贴边收起"
            }
        header.addView(title, LinearLayout.LayoutParams(-1, dp(28)))
        val handle =
            TextView(context).apply {
                textSize = 22f
                gravity = Gravity.CENTER
                visibility = View.GONE
                contentDescription = "展开摇杆"
                setTextColor(Color.rgb(70, 103, 155))
            }
        var initialX = 0
        var initialY = 0
        var touchX = 0f
        var touchY = 0f
        title.setOnTouchListener { _, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    initialX = params.x
                    initialY = params.y
                    touchX = event.rawX
                    touchY = event.rawY
                    release()
                }
                MotionEvent.ACTION_MOVE -> {
                    val bounds = windows.currentWindowMetrics.bounds
                    params.x =
                        (initialX + event.rawX - touchX)
                            .toInt()
                            .coerceIn(0, (bounds.width() - panel.width).coerceAtLeast(0))
                    params.y =
                        (initialY + event.rawY - touchY)
                            .toInt()
                            .coerceIn(0, (bounds.height() - panel.height - dp(32)).coerceAtLeast(0))
                    windows.updateViewLayout(panel, params)
                }
                MotionEvent.ACTION_UP -> {
                    val right = windows.currentWindowMetrics.bounds.width() - panel.width
                    if (params.x <= dp(12) || params.x >= right - dp(12)) {
                        release()
                        val onRight = params.x > right / 2
                        header.visibility = View.GONE
                        stick.visibility = View.GONE
                        label.visibility = View.GONE
                        handle.text = if (onRight) "‹" else "›"
                        handle.visibility = View.VISIBLE
                        params.width = dp(28)
                        params.x =
                            if (onRight) windows.currentWindowMetrics.bounds.width() - dp(28) else 0
                        windows.updateViewLayout(panel, params)
                        handle.setOnClickListener {
                            handle.visibility = View.GONE
                            header.visibility = View.VISIBLE
                            stick.visibility = View.VISIBLE
                            label.visibility = View.VISIBLE
                            params.width = dp(112)
                            params.x =
                                if (onRight) windows.currentWindowMetrics.bounds.width() - dp(112)
                                else 0
                            windows.updateViewLayout(panel, params)
                        }
                    }
                }
            }
            true
        }
        panel.addView(header)
        stick = StickView(context, changed)
        panel.addView(stick, LinearLayout.LayoutParams(-1, dp(88)))
        label =
            TextView(context).apply {
                text = labelText
                textSize = 10f
                gravity = Gravity.CENTER
                setTextColor(Color.rgb(30, 47, 68))
            }
        panel.addView(label)
        panel.addView(handle, LinearLayout.LayoutParams(-1, dp(48)))
        windows.addView(panel, params)
        this.panel = panel
        Log.i(
            TAG,
            "overlay added ${params.width}x${params.height} @ (${params.x},${params.y}) granted=${Settings.canDrawOverlays(context)}",
        )
    }

    fun setLabel(labelText: String) {
        label.text = labelText
    }

    fun reset(message: String? = null) {
        stick.reset()
        if (message != null) label.text = message
    }

    fun close() {
        panel?.let { windows.removeView(it) }
        panel = null
    }

    private companion object {
        const val TAG = "JustLocationJoystick"
    }
}
