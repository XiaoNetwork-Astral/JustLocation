package me.idk.justlocation.joystick

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View
import android.view.animation.DecelerateInterpolator
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

internal class StickView(
    context: Context,
    private val changed: (StickInput) -> Unit,
    private val cancelled: () -> Unit,
) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private var dx = 0f
    private var dy = 0f
    private var pointer = -1
    private var displayedBearing = 0f
    private var targetBearing = 0.0
    private var returnAnimation: ValueAnimator? = null
    private var directionAnimation: ValueAnimator? = null
    private val radius
        get() = minOf(width, height) * 0.48f

    private val travel
        get() = radius * 0.6f

    init {
        contentDescription = context.getString(R.string.joystick_direction)
    }

    fun reset() {
        returnAnimation?.cancel()
        pointer = -1
        dx = 0f
        dy = 0f
        invalidate()
    }

    fun showMotion(input: StickInput) {
        if (input.strength <= 0 || abs(MotionController.delta(targetBearing, input.bearing)) < 0.01)
            return
        targetBearing = input.bearing
        directionAnimation?.cancel()
        val end =
            displayedBearing +
                MotionController.delta(displayedBearing.toDouble(), targetBearing).toFloat()
        directionAnimation =
            ValueAnimator.ofFloat(displayedBearing, end).apply {
                duration = 90
                interpolator = DecelerateInterpolator()
                addUpdateListener {
                    displayedBearing = it.animatedValue as Float
                    invalidate()
                }
                start()
            }
    }

    private fun returnToCenter() {
        returnAnimation?.cancel()
        val x = dx
        val y = dy
        returnAnimation =
            ValueAnimator.ofFloat(1f, 0f).apply {
                duration = 140
                interpolator = DecelerateInterpolator()
                addUpdateListener {
                    val fraction = it.animatedValue as Float
                    dx = x * fraction
                    dy = y * fraction
                    invalidate()
                }
                start()
            }
    }

    override fun onDraw(canvas: Canvas) {
        val cx = width / 2f
        val cy = height / 2f
        val r = radius
        paint.color = Color.argb(175, 30, 30, 30)
        paint.style = Paint.Style.FILL
        canvas.drawCircle(cx, cy, r, paint)
        paint.textSize = r * 0.12f
        paint.textAlign = Paint.Align.CENTER
        for ((index, label) in listOf("N", "E", "S", "W").withIndex()) {
            val angle = index * Math.PI / 2
            val tx = cx + (sin(angle) * r * 0.61).toFloat()
            val ty = cy - (cos(angle) * r * 0.61).toFloat()
            paint.color = Color.WHITE
            canvas.drawText(label, tx, ty - (paint.ascent() + paint.descent()) / 2, paint)
            canvas.save()
            canvas.rotate(index * 90f, cx, cy)
            paint.color = if (index == 0) Color.rgb(255, 87, 93) else Color.WHITE
            val arrow =
                Path().apply {
                    moveTo(cx, cy - r * 0.83f)
                    lineTo(cx - r * 0.055f, cy - r * 0.72f)
                    lineTo(cx + r * 0.055f, cy - r * 0.72f)
                    close()
                }
            canvas.drawPath(arrow, paint)
            canvas.restore()
        }
        // Keep the last direction visible when idle; visual easing never sends movement commands.
        run {
            canvas.save()
            canvas.rotate(displayedBearing, cx, cy)
            paint.color = Color.LTGRAY
            paint.strokeWidth = r * 0.014f
            canvas.drawLine(cx, cy, cx, cy - r * 0.34f, paint)
            paint.color = Color.WHITE
            canvas.drawCircle(cx, cy - r * 0.40f, r * 0.025f, paint)
            val arrow =
                Path().apply {
                    moveTo(cx, cy - r * 0.54f)
                    lineTo(cx - r * 0.055f, cy - r * 0.44f)
                    lineTo(cx + r * 0.055f, cy - r * 0.44f)
                    close()
                }
            canvas.drawPath(arrow, paint)
            canvas.restore()
        }
        paint.color = Color.rgb(248, 248, 248)
        canvas.drawCircle(cx + dx, cy + dy, r * 0.19f, paint)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                returnAnimation?.cancel()
                pointer = event.getPointerId(0)
                parent?.requestDisallowInterceptTouchEvent(true)
            }
            MotionEvent.ACTION_CANCEL -> {
                reset()
                cancelled()
                return true
            }
            MotionEvent.ACTION_UP -> {
                pointer = -1
                changed(StickInput.Still)
                returnToCenter()
                performClick()
                return true
            }
            MotionEvent.ACTION_POINTER_UP ->
                if (event.getPointerId(event.actionIndex) == pointer) {
                    reset()
                    cancelled()
                    return true
                }
        }
        val index = event.findPointerIndex(pointer)
        if (index >= 0) {
            val x = event.getX(index) - width / 2f
            val y = event.getY(index) - height / 2f
            val scale = (travel / hypot(x, y).coerceAtLeast(1f)).coerceAtMost(1f)
            dx = x * scale
            dy = y * scale
            changed(StickInput.from(x, y, travel))
            invalidate()
        }
        return true
    }

    override fun performClick(): Boolean = super.performClick()

    override fun onDetachedFromWindow() {
        returnAnimation?.cancel()
        directionAnimation?.cancel()
        super.onDetachedFromWindow()
    }
}
