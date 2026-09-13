package me.idk.justlocation.probe

import android.annotation.SuppressLint
import android.location.GnssMeasurementsEvent
import android.location.GnssNavigationMessage
import android.location.GnssStatus
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.location.OnNmeaMessageListener
import android.os.CancellationSignal
import android.os.Handler
import android.os.Parcel
import android.os.Parcelable
import android.os.SystemClock
import java.io.File
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.json.JSONObject

/** Location and satellite observations collected off the main thread. */
internal class LocationChecks(private val manager: LocationManager, private val handler: Handler) {
    /** Timestamped, independent platform callbacks for sustained location diagnosis. */
    @SuppressLint("MissingPermission")
    fun collectContinuity(
        report: CheckReport,
        durationMs: Long,
        destination: File,
        includeQueries: Boolean = false,
    ) {
        val samples = java.util.concurrent.ConcurrentLinkedQueue<String>()
        val listeners = ArrayList<LocationListener>()
        val providers =
            mutableListOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)
        if (includeQueries) {
            for (provider in
                listOf(LocationManager.FUSED_PROVIDER, LocationManager.PASSIVE_PROVIDER)) {
                if (provider in manager.allProviders) providers.add(provider)
            }
        }
        destination.writeText("")
        try {
            for (provider in providers) {
                val listener = LocationListener { fix ->
                    samples.add(locationRow("callback", provider, fix).toString())
                }
                manager.requestLocationUpdates(provider, 500L, 0f, listener, handler.looper)
                listeners.add(listener)
            }
            val end = SystemClock.elapsedRealtime() + durationMs
            var nextQuery = 0L
            while (SystemClock.elapsedRealtime() < end) {
                if (includeQueries && SystemClock.elapsedRealtime() >= nextQuery) {
                    for (provider in providers) {
                        val row =
                            try {
                                locationRow(
                                    "last_known",
                                    provider,
                                    manager.getLastKnownLocation(provider),
                                )
                            } catch (error: Exception) {
                                locationRow("last_known", provider, null)
                                    .put("error_class", error.javaClass.name)
                            }
                        samples.add(row.toString())
                    }
                    nextQuery = SystemClock.elapsedRealtime() + 1000
                }
                Thread.sleep(500)
                destination.appendText(
                    buildString {
                        while (true) appendLine(samples.poll() ?: break)
                    }
                )
            }
            report.line("Continuous location capture completed: ${durationMs}ms")
        } finally {
            listeners.forEach { manager.removeUpdates(it) }
            destination.appendText(
                buildString {
                    while (true) appendLine(samples.poll() ?: break)
                }
            )
        }
    }

    private fun locationRow(event: String, provider: String, fix: Location?): JSONObject {
        val row =
            JSONObject()
                .put("event", event)
                .put("provider", provider)
                .put("received_ms", SystemClock.elapsedRealtime())
                .put("present", fix != null)
        if (fix != null) {
            row.put("fix_provider", fix.provider)
                .put("fix_time_ms", fix.time)
                .put("fix_elapsed_ms", fix.elapsedRealtimeNanos / 1_000_000)
                .put("latitude", fix.latitude)
                .put("longitude", fix.longitude)
                .put("accuracy", fix.accuracy)
                .put("speed", fix.speed)
                .put("bearing", fix.bearing)
                .put("mock", fix.isMock)
                .put("extras_satellites", fix.extras?.getInt("satellites", -1) ?: -1)
        }
        return row
    }

    /** Eight seconds of app-side updates for bounded drift and motion acceptance. */
    @SuppressLint("MissingPermission")
    fun collectMovement(report: CheckReport) {
        val samples = CopyOnWriteArrayList<Location>()
        val listener = LocationListener { samples.add(Location(it)) }
        try {
            manager.requestLocationUpdates(
                LocationManager.GPS_PROVIDER,
                500L,
                0f,
                listener,
                handler.looper,
            )
            Thread.sleep(8000)
        } finally {
            manager.removeUpdates(listener)
        }
        val last = samples.lastOrNull()
        val all =
            samples.joinToString("|") {
                String.format(Locale.ROOT, "%.6f,%.6f", it.latitude, it.longitude)
            }
        report.line(
            "gps ${last?.latitude}, ${last?.longitude} source=live seen=${samples.size} all=$all"
        )
    }

    // The activity requests location access; each read also reports runtime failures.
    @SuppressLint("MissingPermission")
    fun collect(report: CheckReport) {
        report.section("location")
        val fresh = requestFixes(manager)
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            val seen = fresh[provider]
            val fix =
                seen?.lastOrNull()
                    ?: try {
                        manager.getLastKnownLocation(provider)
                    } catch (error: Exception) {
                        null
                    }
            val source = if (seen != null) "live" else "cached"
            val where =
                fix?.let { String.format(Locale.ROOT, "%.6f, %.6f", it.latitude, it.longitude) }
                    ?: "none"
            val all =
                seen?.joinToString("|") {
                    String.format(Locale.ROOT, "%.6f,%.6f", it.latitude, it.longitude)
                } ?: "none"
            report.line(
                "$provider $where altitude ${fix?.altitude} accuracy ${fix?.accuracy} mock=${fix?.isMock} source=$source seen=${seen?.size ?: 0} all=$all"
            )
        }

        val satellites = ArrayList<String>()
        val nmea = ArrayList<String>()
        val status =
            object : GnssStatus.Callback() {
                override fun onSatelliteStatusChanged(value: GnssStatus) {
                    synchronized(satellites) {
                        satellites.clear()
                        for (index in 0 until value.satelliteCount) {
                            satellites.add(
                                "${value.getSvid(index)}/${value.getCn0DbHz(index).toInt()}"
                            )
                        }
                    }
                }
            }
        val nmeaListener = OnNmeaMessageListener { message, _ ->
            synchronized(nmea) { if (nmea.size < 200) nmea.add(message.trim()) }
        }
        val measurementCount = AtomicInteger()
        val measurementSats = AtomicInteger(-1)
        val measurementClock = AtomicReference<String>()
        val measurementCallback =
            object : GnssMeasurementsEvent.Callback() {
                override fun onGnssMeasurementsReceived(event: GnssMeasurementsEvent) {
                    measurementCount.incrementAndGet()
                    measurementSats.set(event.measurements.size)
                    val clock = event.clock
                    measurementClock.set(
                        "timeNanos=${clock.timeNanos} fullBiasNanos=${clock.fullBiasNanos}"
                    )
                }
            }
        val messageCount = AtomicInteger()
        val messageStatus = AtomicInteger(Int.MIN_VALUE)
        val messageDetail = AtomicReference<String>()
        val messageCallback =
            object : GnssNavigationMessage.Callback() {
                override fun onGnssNavigationMessageReceived(message: GnssNavigationMessage) {
                    messageCount.incrementAndGet()
                    if (messageDetail.get() == null) {
                        messageDetail.set(
                            "type=${message.type} svid=${message.svid} subframe=${message.submessageId} " +
                                "status=${message.status} length=${message.data?.size}"
                        )
                    }
                }

                override fun onStatusChanged(status: Int) {
                    messageStatus.set(status)
                }
            }
        var measurementRegistered = false
        var messageRegistered = false
        try {
            manager.registerGnssMeasurementsCallback(
                { runnable -> handler.post(runnable) },
                measurementCallback,
            )
            measurementRegistered = true
        } catch (error: Exception) {
            report.line("measurements registration failed ${error.message}")
        }
        try {
            manager.registerGnssNavigationMessageCallback(
                { runnable -> handler.post(runnable) },
                messageCallback,
            )
            messageRegistered = true
        } catch (error: Exception) {
            report.line("navigation message registration failed ${error.message}")
        }
        try {
            manager.registerGnssStatusCallback(status, handler)
        } catch (error: Exception) {
            report.line("status callback registration failed ${error.message}")
        }
        try {
            manager.addNmeaListener(nmeaListener, handler)
        } catch (error: Exception) {
            report.line("NMEA registration failed ${error.message}")
        }
        val gpsRequest = CancellationSignal()
        try {
            manager.getCurrentLocation(
                LocationManager.GPS_PROVIDER,
                gpsRequest,
                { runnable -> handler.post(runnable) },
                {},
            )
        } catch (error: Exception) {
            report.line("single GPS request failed ${error.message}")
        }
        try {
            Thread.sleep(5000)
        } finally {
            gpsRequest.cancel()
            try {
                manager.removeNmeaListener(nmeaListener)
            } catch (_: Exception) {}
            try {
                manager.unregisterGnssStatusCallback(status)
            } catch (_: Exception) {}
            if (measurementRegistered)
                try {
                    manager.unregisterGnssMeasurementsCallback(measurementCallback)
                } catch (_: Exception) {}
            if (messageRegistered)
                try {
                    manager.unregisterGnssNavigationMessageCallback(messageCallback)
                } catch (_: Exception) {}
        }

        report.section("raw gnss")
        report.line(
            "measurements registered=$measurementRegistered seen=${measurementCount.get()} satellites=${measurementSats.get()} " +
                "clock=${measurementClock.get() ?: "none"}"
        )
        report.line(
            "navigation registered=$messageRegistered seen=${messageCount.get()} " +
                "capability=${if (messageStatus.get() == Int.MIN_VALUE) "no callback" else messageStatus.get().toString()} " +
                "first=${messageDetail.get() ?: "none"}"
        )
        report.line(parcelRoundTrip())

        report.section("continuous subscription")
        report.line(continuousUpdates(manager))

        report.section("satellites")
        synchronized(satellites) {
            if (satellites.isEmpty()) report.line("no satellite status callback")
            else {
                report.line("count ${satellites.size}")
                report.line("svid/cn0 ${satellites.joinToString(" ")}")
            }
        }

        report.section("nmea")
        synchronized(nmea) {
            report.line("sentences ${nmea.size}")
            report.line(
                "types ${nmea.map { it.substringAfter('$').take(5) }.distinct().joinToString(" ")}"
            )
            nmea.take(2).forEach { report.line(it) }
        }
    }

    // Keep every delivered fix so transient coordinates remain visible in the report.
    // Denied or unavailable providers count as completed requests without a fix.
    @SuppressLint("MissingPermission")
    private fun requestFixes(
        manager: LocationManager,
        timeoutMs: Long = 6000,
    ): Map<String, List<Location>> {
        val result =
            ConcurrentHashMap<
                String,
                CopyOnWriteArrayList<Location>,
            >()
        val latch = CountDownLatch(2)
        val cancellations = ArrayList<CancellationSignal>()
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            try {
                val cancellation = CancellationSignal()
                cancellations.add(cancellation)
                manager.getCurrentLocation(
                    provider,
                    cancellation,
                    { runnable -> handler.post(runnable) },
                ) { location ->
                    if (location != null)
                        result
                            .computeIfAbsent(provider) {
                                CopyOnWriteArrayList()
                            }
                            .add(location)
                    latch.countDown()
                }
            } catch (error: Exception) {
                latch.countDown()
            }
        }
        try {
            latch.await(timeoutMs, TimeUnit.MILLISECONDS)
        } finally {
            cancellations.forEach { it.cancel() }
        }
        return result
    }

    /** Check local Parcel decoding independently of GNSS callback delivery. */
    @SuppressLint(
        "SoonBlockedPrivateApi"
    ) // This diagnostic reports inaccessible ROM fields as failures.
    private fun parcelRoundTrip(): String {
        val messageType = GnssNavigationMessage::class.java
        return try {
            val constructor = messageType.getDeclaredConstructor()
            constructor.isAccessible = true
            val message = constructor.newInstance()
            val type = messageType.getDeclaredField("mType")
            type.isAccessible = true
            type.setInt(message, 257)
            val parcel = Parcel.obtain()
            try {
                parcel.writeParcelable(message, 0)
                parcel.setDataPosition(0)
                val restored = parcel.readParcelable<Parcelable>(messageType.classLoader)
                "parcel_roundtrip written=${message.javaClass.name} read=${restored?.javaClass?.name ?: "null"} " +
                    "same_type=${restored is GnssNavigationMessage}"
            } finally {
                parcel.recycle()
            }
        } catch (error: Throwable) {
            val cause = generateSequence(error) { it.cause }.last()
            "parcel_roundtrip failed=${cause.javaClass.name}: ${cause.message}"
        }
    }

    // Registration failures are reflected in the requested and received counts.
    @SuppressLint("MissingPermission")
    private fun continuousUpdates(manager: LocationManager): String {
        val seen = CopyOnWriteArrayList<String>()
        val listener = LocationListener { location ->
            if (seen.size < 400) {
                seen.add(
                    String.format(
                        Locale.ROOT,
                        "%.6f,%.6f",
                        location.latitude,
                        location.longitude,
                    )
                )
            }
        }
        var requested = 0
        for (provider in listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)) {
            try {
                manager.requestLocationUpdates(provider, 1000L, 0f, listener, handler.looper)
                requested++
            } catch (error: Exception) {}
        }
        try {
            Thread.sleep(60_000)
        } finally {
            try {
                manager.removeUpdates(listener)
            } catch (_: Exception) {}
        }
        val distinct = seen.distinct()
        return "requested=$requested seen=${seen.size} distinct=${distinct.size} " +
            "first=${seen.firstOrNull() ?: "none"} last=${seen.lastOrNull() ?: "none"} " +
            "all_distinct=${distinct.joinToString("|").ifEmpty { "none" }}"
    }
}
