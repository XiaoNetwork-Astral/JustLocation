package me.idk.justlocation.joystick

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.widget.Toast

/** Briefly enters the foreground so the module can start the joystick service. */
class CallActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (intent?.getBooleanExtra("record", false) == true) {
            try {
                startForegroundService(
                    Intent(this, RouteRecordService::class.java)
                        .setAction(
                            if (intent?.getStringExtra("record_action") == "resume")
                                RouteRecordService.ACTION_RESUME
                            else RouteRecordService.ACTION_START
                        )
                )
            } catch (error: Exception) {
                Log.w(TAG, "recording service start failed", error)
            }
            finishAndRemoveTask()
            return
        }
        val speed = intent?.speedExtra() ?: -1.0
        Log.i(TAG, "wake: speed=$speed canDrawOverlays=${Settings.canDrawOverlays(this)}")
        if (!speed.isFinite() || speed <= 0.0 || speed > 1000.0) {
            Log.w(TAG, "invalid speed, ignoring this wake request")
            finishAndRemoveTask()
            return
        }
        if (!Settings.canDrawOverlays(this)) {
            Log.w(TAG, "missing overlay permission")
            Toast.makeText(
                    this,
                    "Allow JustLoystick to draw over other apps first",
                    Toast.LENGTH_LONG,
                )
                .show()
            startActivity(
                Intent(
                    Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                    Uri.parse("package:$packageName"),
                )
            )
            finishAndRemoveTask()
            return
        }
        try {
            startForegroundService(
                Intent(this, JoystickService::class.java).putExtra("speed", speed)
            )
            Log.i(TAG, "joystick service start requested")
        } catch (error: Exception) {
            Log.w(TAG, "service start failed: ${error.javaClass.simpleName}: ${error.message}")
        }
        finishAndRemoveTask()
    }

    private companion object {
        const val TAG = "JustLocationJoystick"
    }
}
