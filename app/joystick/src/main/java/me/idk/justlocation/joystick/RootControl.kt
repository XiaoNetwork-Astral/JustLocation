package me.idk.justlocation.joystick

import android.location.Location
import org.json.JSONObject

/** Calls are made from the controller worker, never the main thread. */
internal object RootControl {
    private val transport = RootTransport()

    fun request(op: String, speed: Double = 0.0, bearing: Double = 0.0): JSONObject {
        val frame = JSONObject().put("version", 1).put("op", op)
        if (op == "drive") frame.put("speed", speed).put("bearing", bearing)
        return request(frame) { error ->
            when (error) {
                "start location simulation first" -> "start the location simulation first"
                "stop the route before using the joystick" -> "stop the route playback first"
                else -> "move failed; check the module state"
            }
        }
    }

    fun requireStaticSession() {
        val state = request("status")
        check(state.optBoolean("requested_active")) { "start the location simulation first" }
        check(state.isNull("route")) { "stop the route playback first" }
        check(state.optBoolean("location_hook_ready")) { "the location hook is not ready" }
    }

    /** The backend owns distance filtering, point limits and the completed track. */
    fun recordPoint(location: Location, seconds: Double): JSONObject {
        require(location.latitude.isFinite() && location.latitude in -90.0..90.0) {
            "invalid latitude"
        }
        require(location.longitude.isFinite() && location.longitude in -180.0..180.0) {
            "invalid longitude"
        }
        val position =
            JSONObject()
                .put("latitude", location.latitude)
                .put("longitude", location.longitude)
                .put("altitude", location.altitude)
                .put("accuracy", location.accuracy.toDouble())
                .put("speed", location.speed.toDouble())
                .put(
                    "bearing",
                    location.bearing.toDouble().let {
                        // Normalize unavailable bearings to zero.
                        if (it.isFinite() && it >= 0.0) it % 360.0 else 0.0
                    },
                )
        val frame =
            JSONObject()
                .put("version", 1)
                .put("op", "record_point")
                .put("position", position)
                .put("seconds", seconds)
        return request(frame) { it.ifEmpty { "writing the location failed" } }
    }

    private fun request(frame: JSONObject, failure: (String) -> String): JSONObject {
        val reply = transport.exchange(frame.toString())
        val response =
            try {
                JSONObject(reply)
            } catch (_: Exception) {
                throw IllegalStateException("module returned no valid state")
            }
        check(response.optBoolean("ok")) { failure(response.optString("error")) }
        return response.getJSONObject("state")
    }
}
