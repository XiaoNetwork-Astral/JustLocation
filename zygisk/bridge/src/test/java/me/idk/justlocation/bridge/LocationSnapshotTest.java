package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import org.junit.Test;

/**
 * The satellite count a delivered fix carries.
 *
 * `Location` and `org.json` are Android framework classes that host tests cannot exercise, so this
 * covers the rule itself; the bridge's location test and the probe app's device checks verify the
 * assembled `Location` extras on a device.
 */
public class LocationSnapshotTest {
    @Test
    public void onlyAGpsFixFromTheModelCarriesASatelliteCount() {
        assertTrue(LocationSnapshot.carriesSatellites("gps", true));
        // Provider names are matched without case sensitivity.
        assertTrue(LocationSnapshot.carriesSatellites("GPS", true));
        // A network, fused or passive fix is not a satellite solution.
        assertFalse(LocationSnapshot.carriesSatellites("network", true));
        assertFalse(LocationSnapshot.carriesSatellites("fused", true));
        assertFalse(LocationSnapshot.carriesSatellites("passive", true));
        assertFalse(LocationSnapshot.carriesSatellites(null, true));
        // With the satellite model off, no fix claims satellites.
        assertFalse(LocationSnapshot.carriesSatellites("gps", false));
    }

    @Test
    public void theCountComesFromTheSameEpochAsNmea() {
        long timestamp = 1_789_000_000_000L;
        int modeled = LocationSnapshot.satelliteCount(31.2, 121.5, 10, 1.2, 87, timestamp);
        assertTrue("a GPS solution must use at least one satellite", modeled > 0);
        assertTrue("a solution cannot use more than the model tracks", modeled <= 32);
        // A moving position keeps a solution count; it never returns zero or a negative value.
        for (double speed : new double[] {0, 1.2, 5, 12}) {
            int count = LocationSnapshot.satelliteCount(31.2, 121.5, 10, speed, 87, timestamp);
            assertTrue("speed " + speed + " produced " + count, count > 0);
        }
    }
}
