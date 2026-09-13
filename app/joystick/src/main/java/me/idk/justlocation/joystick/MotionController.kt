package me.idk.justlocation.joystick

/** UI and sensor events share this state; only the foreground worker renews backend leases. */
internal class MotionController {
    enum class Mode {
        MANUAL,
        LOCKED,
        FOLLOW,
    }

    private var mode = Mode.MANUAL
    private var input = StickInput.Still
    private var last = StickInput.Still
    private var heading: Double? = null
    private var headingAt = 0L

    @Synchronized fun mode() = mode

    @Synchronized
    fun touch(value: StickInput) {
        if (value.strength > 0) {
            if (mode != Mode.LOCKED) mode = Mode.MANUAL
            input = value
            last = value
        } else if (mode == Mode.MANUAL) input = StickInput.Still
    }

    @Synchronized
    fun lock(): Boolean {
        if (mode == Mode.LOCKED) {
            stop()
            return true
        }
        mode = Mode.LOCKED
        input = last
        return true
    }

    @Synchronized
    fun follow() {
        if (mode == Mode.FOLLOW) {
            stop()
            return
        }
        mode = Mode.FOLLOW
        heading = null
        input = StickInput.Still
    }

    @Synchronized
    fun heading(degrees: Double, now: Long) {
        if (!degrees.isFinite()) return
        heading = heading?.let { normalize(it + delta(it, degrees) * 0.25) } ?: normalize(degrees)
        headingAt = now
    }

    @Synchronized
    fun output(now: Long): StickInput =
        when (mode) {
            Mode.MANUAL,
            Mode.LOCKED -> input
            Mode.FOLLOW ->
                heading
                    ?.takeIf { now >= headingAt && now - headingAt <= 1500 }
                    ?.let { StickInput(1.0, it) } ?: StickInput.Still
        }

    @Synchronized
    fun stop() {
        mode = Mode.MANUAL
        input = StickInput.Still
        last = StickInput.Still
        heading = null
    }

    companion object {
        fun normalize(value: Double) = ((value % 360) + 360) % 360

        fun delta(from: Double, to: Double) = ((normalize(to) - normalize(from) + 540) % 360) - 180
    }
}
