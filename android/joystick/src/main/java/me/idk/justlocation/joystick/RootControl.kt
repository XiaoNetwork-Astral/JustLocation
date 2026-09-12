package me.idk.justlocation.joystick

import android.util.Base64
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Calls are made from the controller worker, never the main thread. */
object RootControl {
    private val timeout = Executors.newSingleThreadScheduledExecutor()

    fun request(op: String, speed: Double = 0.0, bearing: Double = 0.0): JSONObject {
        val frame = JSONObject().put("version", 1).put("op", op)
        if (op == "drive") frame.put("speed", speed).put("bearing", bearing)
        val encoded = Base64.encodeToString(frame.toString().toByteArray(Charsets.UTF_8), Base64.NO_WRAP)
        val process = ProcessBuilder("su", "-c",
            "/data/adb/modules/justlocation/bin/justlocationd request $encoded").redirectErrorStream(true).start()
        val deadline = timeout.schedule({ process.destroyForcibly() }, 4, TimeUnit.SECONDS)
        try {
            val reply = process.inputStream.bufferedReader().use { it.readText() }
            check(process.waitFor() == 0) { "cannot reach the module; check root access and module state" }
            val response = try { JSONObject(reply) } catch (_: Exception) {
                throw IllegalStateException("module returned no valid state")
            }
            check(response.optBoolean("ok")) {
                when (response.optString("error")) {
                    "start location simulation first" -> "start the location simulation first"
                    "stop the route before using the joystick" -> "stop the route playback first"
                    else -> "move failed; check the module state"
                }
            }
            return response.getJSONObject("state")
        } finally { deadline.cancel(false); process.destroy() }
    }

    fun requireStaticSession() {
        val state = request("status")
        check(state.optBoolean("requested_active")) { "start the location simulation first" }
        check(state.isNull("route")) { "stop the route playback first" }
        check(state.optBoolean("location_hook_ready")) { "the location hook is not ready" }
    }

    /**
     * 把一条真实定位交给后台的录制缓冲。
     *
     * 只发位置与相对秒数：抽稀、上限与轨迹格式都由后台决定，设备侧不做第二套判断。
     */
    fun recordPoint(location: android.location.Location, seconds: Double): JSONObject {
        // 先在这里挡掉明显坏掉的读数：后台校验失败会整点丢弃，而点数与耗时都不该被它污染。
        require(location.latitude.isFinite() && location.latitude in -90.0..90.0) { "invalid latitude" }
        require(location.longitude.isFinite() && location.longitude in -180.0..180.0) { "invalid longitude" }
        val position = JSONObject()
            .put("latitude", location.latitude).put("longitude", location.longitude)
            .put("altitude", location.altitude).put("accuracy", location.accuracy.toDouble())
            .put("speed", location.speed.toDouble()).put("bearing", location.bearing.toDouble().let {
                // 系统在方向未知时给 -1，而协议要求 0..360。
                if (it.isFinite() && it >= 0.0) it % 360.0 else 0.0
            })
        val frame = JSONObject().put("version", 1).put("op", "record_point")
            .put("position", position).put("seconds", seconds)
        val encoded = Base64.encodeToString(frame.toString().toByteArray(Charsets.UTF_8), Base64.NO_WRAP)
        val process = ProcessBuilder("su", "-c",
            "/data/adb/modules/justlocation/bin/justlocationd request $encoded").redirectErrorStream(true).start()
        val deadline = timeout.schedule({ process.destroyForcibly() }, 4, TimeUnit.SECONDS)
        try {
            val reply = process.inputStream.bufferedReader().use { it.readText() }
            if (process.waitFor() != 0) throw IllegalStateException("cannot reach the module; check root access and module state")
            val response = try { JSONObject(reply) } catch (_: Exception) {
                throw IllegalStateException("module returned no valid state")
            }
            if (!response.optBoolean("ok")) {
                throw IllegalStateException(response.optString("error").ifEmpty { "writing the location failed" })
            }
            return response.getJSONObject("state")
        } finally { deadline.cancel(false); process.destroy() }
    }
}
