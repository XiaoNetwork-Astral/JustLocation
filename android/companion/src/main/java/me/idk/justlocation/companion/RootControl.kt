package me.idk.justlocation.companion

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
            check(process.waitFor() == 0) { "无法连接模块，请检查 Root 授权和模块状态" }
            val response = try { JSONObject(reply) } catch (_: Exception) {
                throw IllegalStateException("模块没有返回有效状态")
            }
            check(response.optBoolean("ok")) {
                when (response.optString("error")) {
                    "start location simulation first" -> "请先在面板开始位置模拟"
                    "stop the route before using the joystick" -> "请先停止路线播放"
                    else -> "移动失败，请检查模块状态"
                }
            }
            return response.getJSONObject("state")
        } finally { deadline.cancel(false); process.destroy() }
    }

    fun requireStaticSession() {
        val state = request("status")
        check(state.optBoolean("requested_active")) { "请先在面板开始位置模拟" }
        check(state.isNull("route")) { "请先停止路线播放" }
        check(state.optBoolean("location_hook_ready")) { "定位 Hook 尚未就绪" }
    }
}
