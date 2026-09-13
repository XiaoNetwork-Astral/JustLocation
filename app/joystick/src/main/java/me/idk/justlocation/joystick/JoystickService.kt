package me.idk.justlocation.joystick

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.res.Configuration
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.widget.Toast
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.json.JSONObject

/** Foreground lifetime and serialized movement requests for the floating joystick. */
class JoystickService : Service() {
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadScheduledExecutor()
    private val controller = MotionController()
    private lateinit var heading: HeadingTracker
    private var lastState: JSONObject? = null
    private var lastPoll = 0L
    @Volatile private var maximumSpeed = 1.5
    @Volatile private var closed = false
    private var moving = false // Worker thread only.
    private var starting = false // Main thread only.
    private var lastRejectedAt = 0L
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
        heading = HeadingTracker(this, controller::heading)
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
                    "Closing stops joystick movement. Route playback continues independently."
                )
                .setOngoing(true)
                .build(),
        )
        registerReceiver(screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF), RECEIVER_NOT_EXPORTED)
        worker.scheduleWithFixedDelay(
            {
                if (!closed) {
                    val now = SystemClock.elapsedRealtime()
                    val input = controller.output(now)
                    if (input.strength > 0 || moving) send(input)
                    else if (now - lastPoll >= 1000) {
                        lastPoll = now
                        try {
                            publish(RootControl.request("status"))
                        } catch (error: Exception) {
                            disconnected(error)
                        }
                    }
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
        render()
        if (overlay == null && !starting) {
            starting = true
            worker.execute {
                try {
                    val state = RootControl.requireReady()
                    publish(state)
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
        val window =
            JoystickOverlay(this, ::change, ::release, ::action) { speed ->
                maximumSpeed = speed
                render()
            }
        try {
            window.show(maximumSpeed)
            overlay = window
            render()
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

    private fun publish(state: JSONObject) {
        main.post {
            if (!closed) {
                lastState = state
                if (!state.optBoolean("requested_active")) {
                    controller.stop()
                    heading.stop()
                }
                render()
            }
        }
    }

    private fun render() {
        val mode = controller.mode()
        val state = lastState
        val route = state?.optJSONObject("route")
        val details =
            when {
                state?.optBoolean("requested_active") != true ->
                    getString(R.string.simulation_stopped)
                route != null ->
                    getString(
                        R.string.route_progress,
                        getString(
                            if (route.optBoolean("paused")) R.string.pause_route
                            else if (route.optBoolean("completed")) R.string.route_arrived
                            else R.string.route_running
                        ),
                        route.optDouble("distance"),
                        route.optDouble("total_distance"),
                    )
                mode == MotionController.Mode.LOCKED -> getString(R.string.direction_locked)
                mode == MotionController.Mode.FOLLOW ->
                    if (controller.output(SystemClock.elapsedRealtime()).strength > 0)
                        getString(R.string.follow_moving)
                    else getString(R.string.compass_unavailable)
                else -> ""
            }
        overlay?.render(
            mode,
            controller.output(SystemClock.elapsedRealtime()),
            maximumSpeed,
            state?.optBoolean("requested_active") == true,
            state?.optJSONObject("steps")?.optBoolean("enabled") == true,
            state?.optJSONObject("gnss")?.optBoolean("gnss_enabled") == true,
            route != null,
            route?.optBoolean("paused") == true,
            route?.optBoolean("completed") == true,
            details,
        )
    }

    private fun disconnected(error: Exception) {
        moving = false
        controller.stop()
        main.post {
            heading.stop()
            if (!closed) {
                render()
                overlay?.reset(error.message ?: "Connection lost; movement stopped")
            }
        }
    }

    private fun send(input: StickInput) {
        try {
            val state = RootControl.request("drive", input.strength * maximumSpeed, input.bearing)
            moving = input.strength > 0
            publish(state)
        } catch (error: Exception) {
            disconnected(error)
        }
    }

    private fun change(input: StickInput) {
        if (input.strength > 0 && !canMove()) {
            overlay?.reset()
            return
        }
        controller.touch(input)
        if (controller.mode() != MotionController.Mode.FOLLOW) heading.stop()
        if (controller.output(SystemClock.elapsedRealtime()).strength == 0.0 && !closed) {
            worker.execute { if (moving) send(StickInput.Still) }
        }
        render()
    }

    private fun release() {
        controller.stop()
        heading.stop()
        if (!closed) worker.execute { if (moving) send(StickInput.Still) }
        overlay?.reset()
        render()
    }

    private fun canMove(): Boolean {
        val reason =
            when {
                lastState?.optBoolean("requested_active") != true -> R.string.start_first
                lastState?.optJSONObject("route") != null -> R.string.stop_route_first
                else -> return true
            }
        val now = SystemClock.elapsedRealtime()
        if (now - lastRejectedAt >= 2000) {
            lastRejectedAt = now
            report(getString(reason))
        }
        return false
    }

    private fun perform(releaseMovement: Boolean = true, operation: () -> Unit) {
        if (closed) return
        if (releaseMovement) release()
        worker.execute {
            if (closed) return@execute
            try {
                operation()
                publish(RootControl.request("status"))
            } catch (error: Exception) {
                try {
                    publish(RootControl.request("status"))
                } catch (_: Exception) {
                    disconnected(error)
                }
                main.post { if (!closed) report(error.message ?: "Operation failed") }
            }
        }
    }

    private fun library(routes: Boolean) {
        overlay?.loading(routes)
        perform {
            val entries = RootControl.library(routes)
            val items =
                (0 until entries.length()).map { index ->
                    val entry = entries.getJSONObject(index)
                    JoystickOverlay.Entry(
                        entry.getString("id"),
                        entry.optString("name", getString(R.string.unnamed)),
                        if (routes)
                            getString(
                                R.string.route_summary,
                                entry.optInt(
                                    "point_count",
                                    entry.optJSONObject("plan")?.optJSONArray("points")?.length()
                                        ?: 0,
                                ),
                            )
                        else
                            String.format(
                                Locale.ROOT,
                                "%.6f, %.6f",
                                entry.optDouble("latitude"),
                                entry.optDouble("longitude"),
                            ),
                    )
                }
            main.post {
                if (!closed)
                    overlay?.choices(routes, items) { id ->
                        perform {
                            RootControl.useSaved(routes, id)
                            main.post { if (!closed) overlay?.showSettings() }
                        }
                    }
            }
        }
    }

    private fun action(action: JoystickOverlay.Action) {
        when (action) {
            JoystickOverlay.Action.LOCK -> {
                if (!canMove()) return
                heading.stop()
                controller.lock()
                if (controller.mode() == MotionController.Mode.MANUAL) release()
                render()
            }
            JoystickOverlay.Action.FOLLOW -> {
                if (!canMove()) return
                controller.follow()
                if (controller.mode() == MotionController.Mode.FOLLOW) {
                    if (!heading.start()) {
                        release()
                        report(getString(R.string.compass_missing))
                    }
                } else release()
                render()
            }
            JoystickOverlay.Action.PLACES -> library(false)
            JoystickOverlay.Action.ROUTES -> library(true)
            JoystickOverlay.Action.SAVE -> {
                release()
                overlay?.askName { name ->
                    perform {
                        RootControl.saveCurrent(name)
                        main.post {
                            if (!closed)
                                Toast.makeText(this, R.string.saved, Toast.LENGTH_SHORT).show()
                        }
                    }
                }
            }
            JoystickOverlay.Action.PAUSE -> perform { RootControl.request("pause_route") }
            JoystickOverlay.Action.RESUME -> perform { RootControl.request("resume_route") }
            JoystickOverlay.Action.STOP -> perform { RootControl.request("stop") }
            JoystickOverlay.Action.START -> perform { RootControl.startCurrent() }
            JoystickOverlay.Action.STEPS ->
                perform(false) { RootControl.toggleSetting("steps", "enabled") }
            JoystickOverlay.Action.GNSS ->
                perform(false) { RootControl.toggleSetting("gnss", "gnss_enabled") }
            JoystickOverlay.Action.CLOSE -> stopSelf()
        }
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        release()
        overlay?.configurationChanged()
    }

    override fun onDestroy() {
        closed = true
        controller.stop()
        heading.stop()
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
