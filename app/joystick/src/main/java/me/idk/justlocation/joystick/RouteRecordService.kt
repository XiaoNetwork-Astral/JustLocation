package me.idk.justlocation.joystick

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Looper
import android.os.SystemClock
import android.util.AtomicFile
import android.util.Log
import java.io.File
import java.util.concurrent.Executors
import org.json.JSONObject

/** Owns location subscriptions; the worker serializes backend recording operations. */
class RouteRecordService : Service() {
    companion object {
        const val ACTION_START = "me.idk.justlocation.joystick.RECORD_START"
        const val ACTION_STOP = "me.idk.justlocation.joystick.RECORD_STOP"
        const val ACTION_STATE = "me.idk.justlocation.joystick.RECORD_STATE"
        private const val CHANNEL = "route-record"
        private const val NOTIFICATION_ID = 2
    }

    private val worker = Executors.newSingleThreadExecutor()
    private lateinit var manager: LocationManager

    // Worker-owned recording state.
    private var origin = 0L
    private var lastError = ""
    private var lastMessage = ""
    // Main-thread admission guards for commands and location callbacks.
    private var started = false
    private var stopping = false
    private var closed = false

    private val listener =
        object : LocationListener {
            override fun onLocationChanged(location: Location) = record(location)

            override fun onProviderEnabled(provider: String) = Unit

            override fun onProviderDisabled(provider: String) = Unit
        }

    override fun onBind(intent: Intent?) = null

    override fun onCreate() {
        super.onCreate()
        manager = getSystemService(LocationManager::class.java)
        val notifications = getSystemService(NotificationManager::class.java)
        notifications.createNotificationChannel(
            NotificationChannel(CHANNEL, "Route recording", NotificationManager.IMPORTANCE_LOW)
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        try {
            when (intent?.action) {
                ACTION_STOP -> stopRecording()
                else -> beginRecording()
            }
        } catch (error: Exception) {
            finish(error)
        }
        return START_NOT_STICKY
    }

    private fun beginRecording() {
        if (started || stopping || closed) return
        started = true
        startForeground(
            NOTIFICATION_ID,
            Notification.Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_menu_mylocation)
                .setContentTitle("Recording route")
                .setContentText("Recording real movement. Keep location simulation stopped.")
                .setOngoing(true)
                .build(),
        )
        worker.execute {
            try {
                val state = RootControl.request("status")
                check(!state.optBoolean("requested_active")) {
                    "stop the location simulation before recording"
                }
                RootControl.request("record_start")
                origin = SystemClock.elapsedRealtime()
                lastError = ""
                lastMessage = "Recording. Run 'justlocationd record stop' when finished."
                startUpdates()
            } catch (error: Exception) {
                lastError = error.message ?: "cannot start recording"
                Log.w("JustLocationRecorder", "recording startup failed", error)
                lastMessage = ""
                publish()
                stopSelf()
            }
        }
    }

    private fun startUpdates() {
        var started = 0
        for (provider in providers()) {
            try {
                manager.requestLocationUpdates(
                    provider,
                    1000L,
                    0f,
                    listener,
                    Looper.getMainLooper(),
                )
                started++
            } catch (error: SecurityException) {
                lastError = "$provider: location permission denied (${error.message})"
            } catch (error: Exception) {
                lastError = "$provider: ${error.message}"
            }
        }
        if (started == 0) {
            finish(Exception(lastError.ifEmpty { "no location provider is available" }))
            return
        }
        publish()
    }

    private fun record(location: Location) {
        if (closed || stopping) return
        val sample = Location(location)
        val timestamp = SystemClock.elapsedRealtime()
        worker.execute {
            try {
                RootControl.recordPoint(sample, (timestamp - origin) / 1000.0)
                publish()
            } catch (error: Exception) {
                lastError = error.message ?: "writing the location failed"
                publish()
            }
        }
    }

    private fun stopRecording() {
        if (stopping || closed) return
        stopping = true
        worker.execute {
            manager.removeUpdates(listener)
            try {
                val state = RootControl.request("record_stop")
                lastMessage = store(state.optJSONObject("recorded"))
                lastError = ""
            } catch (error: Exception) {
                lastError = error.message ?: "cannot finish recording"
                lastMessage = ""
            }
            publish()
            stopSelf()
        }
    }

    private fun finish(error: Exception) {
        Log.w("JustLocationRecorder", "recording stopped", error)
        lastError = error.message ?: "recording failed"
        manager.removeUpdates(listener)
        publish()
        stopSelf()
    }

    private fun store(recorded: JSONObject?): String {
        if (recorded == null) return "No points recorded"
        val points = recorded.optJSONArray("points")?.length() ?: 0
        val file = File(getExternalFilesDir(null) ?: filesDir, "recorded-route.json")
        file.writeText(recorded.toString())
        return "Recorded $points points; saved to ${file.absolutePath}"
    }

    private fun providers(): List<String> =
        listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER).filter {
            manager.allProviders.contains(it)
        }

    private fun publish() {
        val report = JSONObject().put("error", lastError).put("message", lastMessage)
        val file = AtomicFile(File(filesDir, "recording-state.json"))
        var stream: java.io.FileOutputStream? = null
        try {
            stream = file.startWrite()
            stream.write(report.toString().toByteArray(Charsets.UTF_8))
            file.finishWrite(stream)
        } catch (error: Exception) {
            file.failWrite(stream)
            Log.w("JustLocationRecorder", "cannot publish recorder state", error)
        }
        sendBroadcast(
            Intent(ACTION_STATE).setPackage(packageName).apply {
                putExtra("message", lastMessage)
                putExtra("error", lastError)
            }
        )
    }

    override fun onDestroy() {
        closed = true
        manager.removeUpdates(listener)
        // Startup may still be registering providers on the worker when destruction begins.
        worker.execute { manager.removeUpdates(listener) }
        worker.shutdown()
        super.onDestroy()
    }
}
