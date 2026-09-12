package me.idk.justlocation.joystick

import java.io.IOException
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
        val process =
            try {
                start("/data/adb/modules/justlocation/bin/justlocationd request $encoded")
            } catch (error: IOException) {
                throw IllegalStateException(ROOT_HELP, error)
            }
        val deadline =
            timeout.schedule({ process.destroyForcibly() }, timeoutMillis, TimeUnit.MILLISECONDS)
        try {
            val reply = process.inputStream.bufferedReader().use { it.readText() }
            check(process.waitFor() == 0) {
                if (
                    reply.contains("denied", ignoreCase = true) ||
                        reply.contains("not allowed", ignoreCase = true)
                )
                    ROOT_HELP
                else
                    "cannot reach the module; check that JustLocation is enabled and its service is running. $ROOT_HELP"
            }
            return reply
        } finally {
            deadline.cancel(false)
            process.destroy()
        }
    }

    private companion object {
        const val ROOT_HELP =
            "Root access unavailable. In KernelSU > Superuser, allow JustLoystick (me.idk.justlocation.joystick), then retry."
        val timeout = Executors.newSingleThreadScheduledExecutor()
    }
}
