package me.idk.justlocation.joystick

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.View

/** Small monochrome controls drawn in a 24-unit viewport. */
internal class OverlayIcon(context: Context, initial: Symbol, color: Int) : View(context) {
    enum class Symbol {
        MENU,
        EXPAND,
        COLLAPSE,
        BACK,
        PLUS,
        PLACE,
        ROUTE,
        WALK,
        RUN,
        BIKE,
        CAR,
        PLANE,
    }

    var symbol = initial
        set(value) {
            field = value
            invalidate()
        }

    private val paint =
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            this.color = color
            strokeWidth = 1.8f
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }

    override fun onDraw(canvas: Canvas) {
        val size = minOf(width, height) * 0.68f
        canvas.save()
        canvas.translate((width - size) / 2, (height - size) / 2)
        canvas.scale(size / 24f, size / 24f)
        paint.style = Paint.Style.STROKE
        fun line(vararg coordinates: Float) {
            val path =
                Path().apply {
                    moveTo(coordinates[0], coordinates[1])
                    for (i in 2 until coordinates.size step 2) lineTo(
                        coordinates[i],
                        coordinates[i + 1],
                    )
                }
            canvas.drawPath(path, paint)
        }
        fun dot(x: Float, y: Float, r: Float) {
            paint.style = Paint.Style.FILL
            canvas.drawCircle(x, y, r, paint)
            paint.style = Paint.Style.STROKE
        }
        when (symbol) {
            Symbol.MENU -> {
                line(3f, 7f, 21f, 7f)
                line(3f, 12f, 21f, 12f)
                line(3f, 17f, 21f, 17f)
            }
            Symbol.EXPAND -> {
                line(12f, 3f, 21f, 12f, 12f, 21f, 3f, 12f, 12f, 3f)
                line(12f, 7f, 17f, 12f, 12f, 17f, 7f, 12f, 12f, 7f)
            }
            Symbol.COLLAPSE -> {
                canvas.drawRect(5f, 5f, 19f, 19f, paint)
                canvas.drawRect(9f, 9f, 15f, 15f, paint)
            }
            Symbol.BACK -> {
                line(11f, 4f, 3f, 12f, 11f, 20f)
                line(3f, 12f, 21f, 12f)
            }
            Symbol.PLUS -> {
                line(12f, 3f, 12f, 21f)
                line(3f, 12f, 21f, 12f)
            }
            Symbol.PLACE -> {
                val path =
                    Path().apply {
                        moveTo(12f, 22f)
                        cubicTo(8f, 17f, 4f, 12f, 4f, 8f)
                        cubicTo(4f, -1f, 20f, -1f, 20f, 8f)
                        cubicTo(20f, 12f, 16f, 17f, 12f, 22f)
                    }
                canvas.drawPath(path, paint)
                canvas.drawCircle(12f, 8f, 3f, paint)
            }
            Symbol.ROUTE -> {
                dot(5f, 19f, 2f)
                dot(19f, 5f, 2f)
                line(5f, 16f, 5f, 11f, 19f, 11f, 19f, 8f)
            }
            Symbol.WALK -> {
                dot(13f, 3f, 2f)
                line(11f, 7f, 9f, 14f, 6f, 21f)
                line(9f, 14f, 15f, 17f, 17f, 22f)
                line(7f, 12f, 7f, 8f, 11f, 7f, 15f, 12f, 19f, 12f)
            }
            Symbol.RUN -> {
                dot(16f, 3f, 2f)
                line(14f, 7f, 10f, 13f, 5f, 15f, 2f, 11f)
                line(10f, 13f, 16f, 15f, 14f, 22f)
                line(8f, 10f, 9f, 6f, 14f, 7f, 17f, 11f, 22f, 8f)
            }
            Symbol.BIKE -> {
                canvas.drawCircle(5f, 18f, 4f, paint)
                canvas.drawCircle(19f, 18f, 4f, paint)
                dot(15f, 3f, 2f)
                line(12f, 7f, 9f, 11f, 14f, 14f, 11f, 19f)
                line(12f, 7f, 16f, 10f, 19f, 10f, 19f, 18f)
            }
            Symbol.CAR -> {
                line(3f, 18f, 3f, 10f, 6f, 4f, 18f, 4f, 21f, 10f, 21f, 18f, 3f, 18f)
                line(3f, 10f, 21f, 10f)
                line(5f, 18f, 5f, 21f)
                line(19f, 18f, 19f, 21f)
                dot(7f, 14f, 1.5f)
                dot(17f, 14f, 1.5f)
            }
            Symbol.PLANE -> {
                line(
                    2f,
                    16f,
                    9f,
                    14f,
                    7f,
                    4f,
                    10f,
                    4f,
                    15f,
                    12f,
                    21f,
                    10f,
                    23f,
                    12f,
                    7f,
                    21f,
                    2f,
                    16f,
                )
            }
        }
        canvas.restore()
    }
}
