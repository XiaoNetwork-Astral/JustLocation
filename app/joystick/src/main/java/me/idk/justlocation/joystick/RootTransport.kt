package me.idk.justlocation.joystick

import java.util.Base64
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Blocking root command transport. Call only from a worker thread. */
internal class RootTransport(
    private val start: (String) -> Process = { command ->
        ProcessBuilder("su", "-c", command).redirectErrorStream(true).start()
    },
    private val timeoutMillis: Long = 4000,
) {
    fun exchange(frame: String): String {
        val encoded = Base64.getEncoder().encodeToString(frame.toByteArray(Charsets.UTF_8))
        val process = start("/data/adb/modules/justlocation/bin/justlocationd request $encoded")
        val deadline =
            timeout.schedule({ process.destroyForcibly() }, timeoutMillis, TimeUnit.MILLISECONDS)
        try {
            val reply = process.inputStream.bufferedReader().use { it.readText() }
            check(process.waitFor() == 0) {
                "cannot reach the module; check root access and module state"
            }
            return reply
        } finally {
            deadline.cancel(false)
            process.destroy()
        }
    }

    private companion object {
        val timeout = Executors.newSingleThreadScheduledExecutor()
    }
}
