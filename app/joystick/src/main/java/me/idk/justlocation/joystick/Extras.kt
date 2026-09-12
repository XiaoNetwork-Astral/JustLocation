package me.idk.justlocation.joystick

import android.content.Intent

/** Shell --ef extras are Floats; internal callers may supply other numeric types. */
fun Intent.speedExtra(): Double? =
    when (val value = extras?.get("speed")) {
        is Float -> value.toDouble()
        is Double -> value
        is Number -> value.toDouble()
        else -> null
    }
