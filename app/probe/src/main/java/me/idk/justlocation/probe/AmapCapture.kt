package me.idk.justlocation.probe

import android.content.Context
import android.os.SystemClock
import com.amap.api.location.AMapLocationClient
import com.amap.api.location.AMapLocationClientOption
import java.io.File
import org.json.JSONObject

/** Independent SDK observations; never substitute or filter SDK results. Called on main thread. */
class AmapCapture(private val context: Context) : AutoCloseable {
    private val destination = File(context.filesDir, "amap-continuity.jsonl")
    private var client: AMapLocationClient? = null

    fun start(mode: String, cache: Boolean) {
        destination.writeText("")
        try {
            check(
                context
                    .getSharedPreferences("amap_diagnostics", Context.MODE_PRIVATE)
                    .getBoolean("consent", false)
            ) {
                "AMap privacy consent required"
            }
            val key = File(context.filesDir, "amap-key.txt").readText().trim()
            require(key.matches(Regex("[0-9a-fA-F]{32}"))) { "Invalid Android key file" }
            val selected =
                when (mode) {
                    "gps" -> AMapLocationClientOption.AMapLocationMode.Device_Sensors
                    "network" -> AMapLocationClientOption.AMapLocationMode.Battery_Saving
                    "high" -> AMapLocationClientOption.AMapLocationMode.Hight_Accuracy
                    else -> error("Unknown AMap mode")
                }
            AMapLocationClient.updatePrivacyShow(context, true, true)
            AMapLocationClient.updatePrivacyAgree(context, true)
            AMapLocationClient.setApiKey(key)
            val options =
                AMapLocationClientOption()
                    .setLocationMode(selected)
                    .setInterval(1000)
                    .setOnceLocation(false)
                    .setNeedAddress(false)
                    .setOffset(false)
                    .setLocationCacheEnable(cache)
            client =
                AMapLocationClient(context.applicationContext).also { active ->
                    active.setLocationOption(options)
                    active.setLocationListener { fix ->
                        val row =
                            JSONObject().put("event", "fix").put("mode", mode).put("cache", cache)
                        if (fix != null) {
                            row.put("error_code", fix.errorCode)
                                .put("error_info", fix.errorInfo)
                                .put("location_type", fix.locationType)
                                .put("provider", fix.provider)
                                .put("latitude", fix.latitude)
                                .put("longitude", fix.longitude)
                                .put("accuracy", fix.accuracy)
                                .put("speed", fix.speed)
                                .put("fix_time_ms", fix.time)
                                .put("fix_elapsed_ms", fix.elapsedRealtimeNanos / 1_000_000)
                                .put("coord_type", fix.coordType)
                                .put("satellites", fix.satellites)
                                .put("gps_accuracy_status", fix.gpsAccuracyStatus)
                                .put("mock", fix.isMock)
                        } else row.put("error_code", -1)
                        write(row)
                    }
                    write(
                        JSONObject()
                            .put("event", "start")
                            .put("mode", mode)
                            .put("cache", cache)
                            .put("sdk_version", active.version)
                            // The consumer identity matters when a result is compared with what the
                            // system providers delivered to the same ordinary application.
                            .put("app_package", context.packageName)
                            .put("app_version", appVersion())
                            .put("api_level", android.os.Build.VERSION.SDK_INT)
                    )
                    active.startLocation()
                }
        } catch (error: Exception) {
            write(JSONObject().put("event", "error").put("error_class", error.javaClass.name))
            close()
        }
    }

    @Synchronized
    private fun write(row: JSONObject) {
        destination.appendText(
            row.put("received_ms", SystemClock.elapsedRealtime()).toString() + "\n"
        )
    }

    private fun appVersion(): String =
        try {
            val info = context.packageManager.getPackageInfo(context.packageName, 0)
            "${info.versionName} (${info.longVersionCode})"
        } catch (_: Exception) {
            "unknown"
        }

    override fun close() {
        client?.stopLocation()
        client?.onDestroy()
        client = null
    }
}
