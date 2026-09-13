package me.idk.justlocation.joystick

import org.junit.Assert.*
import org.junit.Test

class MotionControllerTest {
    @Test
    fun lockedReleaseKeepsDirectionButUnlockAndStopEndMovement() {
        val model = MotionController()
        assertTrue(model.lock())
        assertEquals(StickInput.Still, model.output(0))
        model.touch(StickInput(0.7, 90.0))
        model.touch(StickInput.Still)
        assertEquals(StickInput(0.7, 90.0), model.output(999999))
        model.touch(StickInput(0.8, 180.0))
        model.touch(StickInput.Still)
        assertEquals(StickInput(0.8, 180.0), model.output(999999))
        model.lock()
        assertEquals(StickInput.Still, model.output(999999))
        assertTrue(model.lock())
        assertEquals(StickInput.Still, model.output(999999))
    }

    @Test
    fun followUsesShortestAngleAndStopsWhenSamplesExpire() {
        val model = MotionController()
        model.follow()
        assertEquals(StickInput.Still, model.output(0))
        model.heading(359.0, 100)
        model.heading(1.0, 200)
        assertEquals(359.5, model.output(200).bearing, 1e-9)
        assertEquals(1.0, model.output(1700).strength, 0.0)
        assertEquals(StickInput.Still, model.output(1701))
        model.touch(StickInput(0.4, 180.0))
        assertEquals(MotionController.Mode.MANUAL, model.mode())
        assertEquals(StickInput(0.4, 180.0), model.output(2000))
        model.stop()
        assertEquals(StickInput.Still, model.output(2000))
    }
}
