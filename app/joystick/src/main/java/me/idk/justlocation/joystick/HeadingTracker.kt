package me.idk.justlocation.joystick

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.SystemClock
import android.view.Surface
import android.view.WindowManager
import kotlin.math.hypot

/**
 * Screen-top heading relative to magnetic north; no declination inferred from a simulated point.
 */
internal class HeadingTracker(context: Context, private val update: (Double, Long) -> Unit) :
    SensorEventListener {
    private val sensors = context.getSystemService(SensorManager::class.java)
    private val windows = context.getSystemService(WindowManager::class.java)
    private val rotation = FloatArray(9)
    private val remapped = FloatArray(9)
    private val angles = FloatArray(3)
    private var active = false

    fun start(): Boolean {
        if (active) return true
        val sensor =
            sensors.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)
                ?: sensors.getDefaultSensor(Sensor.TYPE_GEOMAGNETIC_ROTATION_VECTOR)
                ?: return false
        active = sensors.registerListener(this, sensor, SensorManager.SENSOR_DELAY_UI)
        return active
    }

    fun stop() {
        sensors.unregisterListener(this)
        active = false
    }

    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit

    @Suppress("DEPRECATION")
    override fun onSensorChanged(event: SensorEvent) {
        if (!active || event.accuracy <= SensorManager.SENSOR_STATUS_UNRELIABLE) return
        SensorManager.getRotationMatrixFromVector(rotation, event.values)
        val (x, y) =
            when (windows.defaultDisplay.rotation) {
                Surface.ROTATION_90 -> SensorManager.AXIS_Y to SensorManager.AXIS_MINUS_X
                Surface.ROTATION_180 -> SensorManager.AXIS_MINUS_X to SensorManager.AXIS_MINUS_Y
                Surface.ROTATION_270 -> SensorManager.AXIS_MINUS_Y to SensorManager.AXIS_X
                else -> SensorManager.AXIS_X to SensorManager.AXIS_Y
            }
        if (!SensorManager.remapCoordinateSystem(rotation, x, y, remapped)) return
        // The screen's top edge has no useful compass projection when held nearly vertical.
        if (hypot(remapped[1], remapped[4]) < 0.1f) return
        SensorManager.getOrientation(remapped, angles)
        update(Math.toDegrees(angles[0].toDouble()), SystemClock.elapsedRealtime())
    }
}
