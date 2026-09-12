package me.idk.justlocation.joystick

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Toast
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/** Foreground lifetime and serialized movement requests for the floating joystick. */
class JoystickService : Service() {
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadScheduledExecutor()
    private val desired = AtomicReference(StickInput.Still)
    @Volatile private var maximumSpeed = 1.5
    @Volatile private var closed = false
    private var moving = false // Worker thread only.
    private var starting = false // Main thread only.
    private var overlay: JoystickOverlay? = null
    private val screenOff =
        object : BroadcastReceiver() {
            override fun onReceive(context: Context?, intent: Intent?) {
                release()
            }
        }

    override fun onBind(intent: Intent?) = null

    override fun onCreate() {
        super.onCreate()
        val notifications = getSystemService(NotificationManager::class.java)
        notifications.createNotificationChannel(
            NotificationChannel(CHANNEL, "Floating joystick", NotificationManager.IMPORTANCE_LOW)
        )
        startForeground(
            1,
            Notification.Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_menu_mylocation)
                .setContentTitle("Floating joystick is open")
                .setContentText(
                    "Release to hold position. Closing the joystick keeps simulation running."
                )
                .setOngoing(true)
                .build(),
        )
        registerReceiver(screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF), RECEIVER_NOT_EXPORTED)
        worker.scheduleWithFixedDelay(
            {
                if (!closed) {
                    val input = desired.get()
                    if (input.strength > 0 || moving) send(input)
                }
            },
            0,
            500,
            TimeUnit.MILLISECONDS,
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val speed = intent?.speedExtra() ?: 1.5
        if (!speed.isFinite() || speed <= 0 || speed > 1000) {
            report("invalid speed: $speed")
            stopSelf()
            return START_NOT_STICKY
        }
        maximumSpeed = speed
        overlay?.setLabel(speedLabel())
        if (overlay == null && !starting) {
            starting = true
            worker.execute {
                try {
                    RootControl.requireStaticSession()
                    main.post {
                        if (!closed) showOverlay()
                    }
                } catch (error: Exception) {
                    main.post {
                        if (!closed) {
                            starting = false
                            report(
                                "session not ready: ${error.javaClass.simpleName}: ${error.message}"
                            )
                            stopSelf()
                        }
                    }
                }
            }
        }
        return START_NOT_STICKY
    }

    private fun showOverlay() {
        starting = false
        val window = JoystickOverlay(this, ::change, ::release)
        try {
            window.show(speedLabel())
            overlay = window
        } catch (error: Exception) {
            window.close()
            report(
                "cannot show the joystick; check the overlay permission (${error.javaClass.simpleName}: ${error.message})"
            )
            stopSelf()
        }
    }

    private fun report(message: String) {
        Log.w(TAG, message)
        Toast.makeText(this, message, Toast.LENGTH_LONG).show()
    }

    private fun speedLabel() = String.format(Locale.ROOT, "Max %.1f km/h", maximumSpeed * 3.6)

    private fun send(input: StickInput) {
        try {
            RootControl.request("drive", input.strength * maximumSpeed, input.bearing)
            moving = input.strength > 0
            main.post { if (!closed) overlay?.setLabel(speedLabel()) }
        } catch (error: Exception) {
            moving = false
            desired.set(StickInput.Still)
            main.post {
                if (!closed) overlay?.reset(error.message ?: "Connection lost; movement stopped")
            }
        }
    }

    private fun change(input: StickInput) {
        desired.set(input)
        if (input.strength == 0.0 && !closed) {
            worker.execute { if (moving) send(StickInput.Still) }
        }
    }

    private fun release() {
        change(StickInput.Still)
        overlay?.reset()
    }

    override fun onDestroy() {
        closed = true
        desired.set(StickInput.Still)
        unregisterReceiver(screenOff)
        overlay?.close()
        overlay = null
        // Serialize release after any in-flight direction. The backend lease also covers process
        // death.
        worker.execute { if (moving) send(StickInput.Still) }
        worker.shutdown()
        super.onDestroy()
    }

    private companion object {
        const val TAG = "JustLocationJoystick"
        const val CHANNEL = "joystick"
    }
}
