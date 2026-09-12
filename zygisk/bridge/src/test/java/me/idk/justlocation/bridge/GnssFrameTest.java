package me.idk.justlocation.bridge;

import static org.junit.Assert.*;

import java.time.Instant;
import java.util.Locale;

import org.junit.Test;

public class GnssFrameTest {
    @Test
    public void sentencesShareUtcPositionSpeedAndSatelliteSet() {
        Locale original = Locale.getDefault();
        try {
            Locale.setDefault(Locale.GERMANY);
            var frame = new GnssFrame(-31.5, -121.25, 12.5, 10, 90,
                    Instant.parse("2026-09-10T23:59:59.125Z").toEpochMilli());
            var lines = frame.nmea();
            assertTrue(
                    lines.get(0).startsWith("$GPGGA,235959.125,3130.00000,S,12115.00000,W,1,08,"));
            assertTrue(
                    lines.get(1).contains(",A,3130.00000,S,12115.00000,W,19.438,90.000,100926,"));
            assertEquals(8, frame.satellites().size());
            assertEquals(2, lines.stream().filter(line -> line.startsWith("$GPGSV")).count());
            assertTrue(lines.stream()
                            .filter(line -> line.startsWith("$GPGSA"))
                            .findFirst()
                            .get()
                            .contains(",03,07,11,14,19,22,26,30,,,,,"));
            for (String line : lines) {
                assertTrue(line.endsWith("\r\n"));
                int star = line.indexOf('*'), checksum = 0;
                for (int i = 1; i < star; i++)
                    checksum ^= line.charAt(i);
                assertEquals(checksum, Integer.parseInt(line.substring(star + 1, star + 3), 16));
            }
        } finally {
            Locale.setDefault(original);
        }
    }

    @Test
    public void roundedMinutesCarryIntoDegreesAndMidnightChangesTheDate() {
        var frame = new GnssFrame(89.999999999, 179.999999999, 0, 0, 0,
                Instant.parse("2026-09-11T00:00:00Z").toEpochMilli());
        assertTrue(frame.nmea().get(0).contains(",9000.00000,N,18000.00000,E,"));
        assertTrue(frame.nmea().get(1).contains(",110926,"));
        assertThrows(IllegalArgumentException.class, () -> new GnssFrame(91, 0, 0, 0, 0, 0));
        assertThrows(IllegalArgumentException.class, () -> new GnssFrame(0, 0, 0, -1, 0, 0));
    }
}
