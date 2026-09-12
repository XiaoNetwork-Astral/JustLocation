package me.idk.justlocation.probe

import android.location.GnssMeasurementsEvent
import android.location.GnssNavigationMessage
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Handler
import android.os.SystemClock
import java.util.Collections
import java.util.concurrent.atomic.AtomicInteger

/** Raw observations only. The independent host receiver validates and solves captured data. */
internal class GnssChecks(private val manager: LocationManager, private val handler: Handler) {
    fun collect(report: CheckReport, duration: Long) {
        val lines = Collections.synchronizedList(mutableListOf<String>())
        val sequence = AtomicInteger()
        val listener = LocationListener { point: Location ->
            lines.add(
                "GNSS_FIX elapsed=${point.elapsedRealtimeNanos} lat=${point.latitude} lon=${point.longitude} alt=${point.altitude}"
            )
        }
        fun callback(name: String) =
            object : GnssNavigationMessage.Callback() {
                override fun onGnssNavigationMessageReceived(message: GnssNavigationMessage) {
                    val hex = message.data.joinToString("") { "%02x".format(it.toInt() and 255) }
                    lines.add(
                        "GNSS_NAV receiver=$name elapsed=${SystemClock.elapsedRealtimeNanos()} prn=${message.svid} type=${message.type} status=${message.status} sf=${message.submessageId} page=${message.messageId} data=$hex"
                    )
                }

                override fun onStatusChanged(status: Int) {
                    lines.add("GNSS_STATUS receiver=$name value=$status")
                }
            }
        val first = callback("A")
        val second = callback("B")
        val raw =
            object : GnssMeasurementsEvent.Callback() {
                override fun onGnssMeasurementsReceived(event: GnssMeasurementsEvent) {
                    val n = sequence.incrementAndGet()
                    val c = event.clock
                    lines.add(
                        "GNSS_CLOCK epoch=$n time=${c.timeNanos} fullBias=${c.fullBiasNanos} bias=${c.biasNanos} discontinuities=${c.hardwareClockDiscontinuityCount}"
                    )
                    for (m in event.measurements) {
                        lines.add(
                            "GNSS_RAW epoch=$n prn=${m.svid} constellation=${m.constellationType} tx=${m.receivedSvTimeNanos} offset=${m.timeOffsetNanos} rate=${m.pseudorangeRateMetersPerSecond} state=${m.state}"
                        )
                    }
                }
            }
        var gps = false
        var a = false
        var b = false
        var measurement = false
        try {
            manager.requestLocationUpdates(
                LocationManager.GPS_PROVIDER,
                1000,
                0f,
                listener,
                handler.looper,
            )
            gps = true
            a = manager.registerGnssNavigationMessageCallback({ handler.post(it) }, first)
            b = manager.registerGnssNavigationMessageCallback({ handler.post(it) }, second)
            measurement = manager.registerGnssMeasurementsCallback({ handler.post(it) }, raw)
            lines.add("GNSS_REGISTER a=$a b=$b measurements=$measurement")
            Thread.sleep(duration.coerceIn(8000, 120000))
        } finally {
            if (a) manager.unregisterGnssNavigationMessageCallback(first)
            if (b) manager.unregisterGnssNavigationMessageCallback(second)
            if (measurement) manager.unregisterGnssMeasurementsCallback(raw)
            if (gps) manager.removeUpdates(listener)
        }
        // Allow any already queued callbacks to drain, then verify no continuing synthetic feed.
        Thread.sleep(1000)
        val after = lines.size
        Thread.sleep(2000)
        report.section("gnss capture")
        synchronized(lines) { lines.forEach { report.line(it) } }
        report.line("GNSS_UNREGISTER additional=${lines.size - after}")
    }
}
