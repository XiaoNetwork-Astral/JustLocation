package me.idk.justlocation.companion

import org.junit.Assert.*
import org.junit.Test

class StickInputTest {
    @Test fun cardinalDirectionsAndClampedSpeed() {
        assertEquals(0.0, StickInput.from(0f, -100f, 100f).bearing, 0.001)
        assertEquals(90.0, StickInput.from(100f, 0f, 100f).bearing, 0.001)
        assertEquals(180.0, StickInput.from(0f, 100f, 100f).bearing, 0.001)
        assertEquals(270.0, StickInput.from(-100f, 0f, 100f).bearing, 0.001)
        assertEquals(1.0, StickInput.from(300f, 400f, 100f).strength, 0.001)
    }
    @Test fun centerIsStillAndDistanceControlsSpeed() {
        assertEquals(0.0, StickInput.from(5f, 5f, 100f).strength, 0.0)
        assertEquals(0.5, StickInput.from(56f, 0f, 100f).strength, 0.001)
        assertEquals(0.0, StickInput.from(0f, 0f, 0f).strength, 0.0)
    }
}
