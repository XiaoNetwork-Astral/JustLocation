package me.idk.justlocation.joystick

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.view.MotionEvent
import android.view.View
import kotlin.math.hypot

internal class StickView(context: Context, val changed: (StickInput) -> Unit) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private var dx = 0f
    private var dy = 0f
    private var pointer = -1
    private val radius
        get() = minOf(width, height) * 0.36f

    init {
        contentDescription = "Directional joystick"
    }

    fun reset() {
        pointer = -1
        dx = 0f
        dy = 0f
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        paint.color = Color.rgb(220, 230, 243)
        canvas.drawCircle(width / 2f, height / 2f, radius + 12, paint)
        paint.color = Color.rgb(169, 190, 217)
        paint.strokeWidth = 2f
        canvas.drawLine(width / 2f - radius, height / 2f, width / 2f + radius, height / 2f, paint)
        canvas.drawLine(width / 2f, height / 2f - radius, width / 2f, height / 2f + radius, paint)
        paint.color = Color.rgb(70, 103, 155)
        canvas.drawCircle(width / 2f + dx, height / 2f + dy, radius * 0.38f, paint)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> pointer = event.getPointerId(0)
            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_CANCEL -> {
                reset()
                changed(StickInput.Still)
                return true
            }
            MotionEvent.ACTION_POINTER_UP ->
                if (event.getPointerId(event.actionIndex) == pointer) {
                    reset()
                    changed(StickInput.Still)
                    return true
                }
        }
        val index = event.findPointerIndex(pointer)
        if (index >= 0) {
            val x = event.getX(index) - width / 2f
            val y = event.getY(index) - height / 2f
            val scale = (radius / hypot(x, y).coerceAtLeast(1f)).coerceAtMost(1f)
            dx = x * scale
            dy = y * scale
            changed(StickInput.from(x, y, radius))
            invalidate()
        }
        return true
    }
}
