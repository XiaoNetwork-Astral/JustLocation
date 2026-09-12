package me.idk.justlocation.probe

import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Collect actual application-side events; the integration runner evaluates them. */
internal class StepChecks(private val manager: SensorManager, private val handler: Handler) {
    fun collect(report: CheckReport) {
        val readings = ArrayList<String>()
        val registered = CountDownLatch(1)
        val listener =
            object : SensorEventListener {
                override fun onSensorChanged(event: SensorEvent) {
                    synchronized(readings) {
                        readings.add(
                            "step_event ${event.sensor.type} ${event.values[0]} ${event.timestamp}"
                        )
                    }
                }

                override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {}
            }
        try {
            handler.post {
                try {
                    for (type in listOf(Sensor.TYPE_STEP_COUNTER, Sensor.TYPE_STEP_DETECTOR)) {
                        val sensor = manager.getDefaultSensor(type)
                        val success =
                            sensor != null &&
                                manager.registerListener(
                                    listener,
                                    sensor,
                                    SensorManager.SENSOR_DELAY_NORMAL,
                                    0,
                                    handler,
                                )
                        synchronized(readings) { readings.add("step_registered $type $success") }
                    }
                } catch (error: Throwable) {
                    synchronized(readings) { readings.add("step_error $error") }
                } finally {
                    registered.countDown()
                }
            }
            if (!registered.await(5, TimeUnit.SECONDS))
                throw IllegalStateException("step registration timed out")
            Thread.sleep(12_000)
        } finally {
            // SensorManager is thread-safe; unregister before publishing the final copy.
            manager.unregisterListener(listener)
        }
        report.section("steps")
        synchronized(readings) { readings.forEach(report::line) }
    }
}
