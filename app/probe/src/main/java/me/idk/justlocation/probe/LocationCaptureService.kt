package me.idk.justlocation.probe

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.location.LocationManager
import android.os.Handler
import android.os.Looper
import android.util.Log
import java.io.File
import java.util.concurrent.Executors

/** Keep the observer eligible for location while the target app is in front. */
class LocationCaptureService : Service() {
    private val worker = Executors.newSingleThreadExecutor()
    private var running = false
    private var amap: AmapCapture? = null

    override fun onBind(intent: Intent?) = null

    override fun onCreate() {
        super.onCreate()
        getSystemService(NotificationManager::class.java)
            .createNotificationChannel(
                NotificationChannel(
                    "location_capture",
                    "Location diagnostics",
                    NotificationManager.IMPORTANCE_LOW,
                )
            )
        startForeground(
            2,
            Notification.Builder(this, "location_capture")
                .setSmallIcon(android.R.drawable.ic_menu_mylocation)
                .setContentTitle("Recording location diagnostics")
                .setContentText("GPS and network callbacks are saved locally in the test app.")
                .build(),
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (running) return START_NOT_STICKY
        running = true
        val duration =
            (intent?.getLongExtra("duration_ms", 180000) ?: 180000).coerceIn(1000, 600000)
        intent?.getStringExtra("amap_mode")?.let { mode ->
            amap = AmapCapture(this)
            amap?.start(mode, intent.getBooleanExtra("amap_cache", false))
        }
        worker.execute {
            try {
                val report = CheckReport()
                LocationChecks(
                        getSystemService(LocationManager::class.java),
                        Handler(Looper.getMainLooper()),
                    )
                    .collectContinuity(
                        report,
                        duration,
                        File(filesDir, "location-continuity.jsonl"),
                        intent?.getBooleanExtra("location_queries", false) == true,
                    )
                Log.i("JustLocationCheck", report.render())
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
            } catch (error: Exception) {
                Log.e("JustLocationCheck", "Cannot capture continuous location", error)
            } finally {
                stopSelf()
            }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        amap?.close()
        worker.shutdownNow()
        super.onDestroy()
    }
}
