package me.idk.justlocation.joystick

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.util.Base64
import java.util.concurrent.CountDownLatch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class RootTransportTest {
    @Test
    fun encodesOneUtf8ArgumentAndClosesTheProcess() {
        val frame = "{\"op\":\"status\",\"text\":\"测试 ' ; $\"}"
        val process = FakeProcess(ByteArrayInputStream("reply\n".toByteArray()))
        var command = ""
        val transport =
            RootTransport(
                start = {
                    command = it
                    process
                }
            )

        assertEquals("reply\n", transport.exchange(frame))
        val prefix = "/data/adb/modules/justlocation/bin/justlocationd request "
        assertTrue(command.startsWith(prefix))
        val argument = command.removePrefix(prefix)
        assertFalse(argument.any { it.isWhitespace() })
        assertEquals(frame, String(Base64.getDecoder().decode(argument), Charsets.UTF_8))
        assertTrue(process.destroyed)
        assertFalse(process.forced)
    }

    @Test
    fun rejectsAnUnsuccessfulCommandAndClosesTheProcess() {
        val process = FakeProcess(ByteArrayInputStream("permission denied".toByteArray()), 1)
        val transport = RootTransport(start = { process })
        val error = assertThrows(IllegalStateException::class.java) { transport.exchange("{}") }
        assertTrue(error.message!!.contains("cannot reach the module"))
        assertTrue(process.destroyed)
    }

    @Test
    fun closesTheProcessWhenReadingFails() {
        val stream =
            object : InputStream() {
                override fun read(): Int = throw IOException("read failed")
            }
        val process = FakeProcess(stream)
        val transport = RootTransport(start = { process })
        assertThrows(IOException::class.java) { transport.exchange("{}") }
        assertTrue(process.destroyed)
    }

    @Test(timeout = 5000)
    fun timeoutTerminatesAProcessBlockedWhileReading() {
        val stopped = CountDownLatch(1)
        val stream =
            object : InputStream() {
                override fun read(): Int {
                    stopped.await()
                    return -1
                }
            }
        val process = FakeProcess(stream, 1, stopped)
        val transport = RootTransport(start = { process }, timeoutMillis = 20)
        assertThrows(IllegalStateException::class.java) { transport.exchange("{}") }
        assertTrue(process.forced)
        assertTrue(process.destroyed)
    }

    private class FakeProcess(
        private val reply: InputStream,
        private val status: Int = 0,
        private val stopped: CountDownLatch? = null,
    ) : Process() {
        @Volatile var destroyed = false
        @Volatile var forced = false

        override fun getInputStream(): InputStream = reply

        override fun getErrorStream(): InputStream = ByteArrayInputStream(byteArrayOf())

        override fun getOutputStream() = ByteArrayOutputStream()

        override fun waitFor(): Int = status

        override fun exitValue(): Int = status

        override fun destroy() {
            destroyed = true
            stopped?.countDown()
        }

        override fun destroyForcibly(): Process {
            forced = true
            stopped?.countDown()
            return this
        }
    }
}
