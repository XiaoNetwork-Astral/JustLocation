package me.idk.justlocation.companion

import kotlin.math.atan2
import kotlin.math.hypot

data class StickInput(val strength: Double, val bearing: Double) {
    companion object {
        val Still = StickInput(0.0, 0.0)
        fun from(dx: Float, dy: Float, radius: Float): StickInput {
            if (radius <= 0f) return Still
            val distance = hypot(dx.toDouble(), dy.toDouble()) / radius
            if (distance <= 0.12) return Still
            return StickInput(((distance.coerceAtMost(1.0) - 0.12) / 0.88),
                (Math.toDegrees(atan2(dx.toDouble(), -dy.toDouble())) + 360) % 360)
        }
    }
}
